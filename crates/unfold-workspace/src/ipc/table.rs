//! The command table: every command's name, argument type, result type, stream message type,
//! and handler, in one list (docs/ipc.md#how-a-command-travels, generated command types).
//!
//! [`Request`] is the serde-tagged request (`{"command": "<name>", "args": {…}}`) the addon's
//! `dispatch` receives; [`Request::run`] calls the handler and serializes its result. With the
//! `ts` feature, the same list feeds `specs`, from which `cargo xtask gen-ipc` writes the UI's
//! `Commands` map.
//!
//! **Adding a command** (see `docs/ipc.md`): write the handler and its argument and result types
//! (with `ts_rs::TS` behind the `ts` feature) in a `commands` module, add one line here, and run
//! `cargo xtask gen-ipc`.

use serde::Deserialize;

use crate::commands::{
    NoArgs, ProjectArgs, ProjectKeyArgs, ProjectPathArgs, boards, code, compiling, entrances, files, graph,
    projects, relations, search, settings, types, windows, workspace,
};
use crate::compile::CompileOutcome;
#[cfg(feature = "ts")]
use crate::compile::CompileProgress;
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
#[cfg(feature = "ts")]
use crate::ipc::typescript::{Collector, CommandSpec, ts_name};
use crate::problems::ProblemItem;
use crate::projects::{AddedProject, WorkspaceView};
use crate::state::{AppSettings, WindowState};

#[cfg(feature = "ts")]
macro_rules! stream_name {
    () => {
        None
    };
    ($stream:ty) => {
        Some(ts_name::<$stream> as fn(&ts_rs::Config) -> String)
    };
}

macro_rules! command_table {
    ($(
        $(#[doc = $doc:literal])+
        $name:literal => $variant:ident($args:ty) -> $result:ty $(, stream $stream:ty)? = $handler:path;
    )+) => {
        /// One command request: `{"command": "<name>", "args": {…}}`. See the
        /// [module documentation](self).
        #[derive(Debug, Deserialize)]
        #[serde(tag = "command", content = "args")]
        pub enum Request {
            $(
                $(#[doc = $doc])+
                #[serde(rename = $name)]
                $variant($args),
            )+
        }

        impl Request {
            /// Every command name, in table order.
            pub const NAMES: &'static [&'static str] = &[$($name),+];

            /// The command's name.
            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name,)+
                }
            }

            /// Runs the command's handler and serializes its result, of the table's result type.
            pub(crate) fn run(self, ctx: &Ctx<'_>) -> CommandResult<String> {
                match self {
                    $(Self::$variant(args) => crate::host::encode::<$result>(&$handler(ctx, args)?),)+
                }
            }
        }

        /// The command table for TypeScript generation, in table order.
        #[cfg(feature = "ts")]
        pub fn specs() -> Vec<CommandSpec> {
            vec![$(
                CommandSpec {
                    name: $name,
                    docs: concat!($($doc, "\n"),+),
                    args: ts_name::<$args>,
                    result: ts_name::<$result>,
                    stream: stream_name!($($stream)?),
                    visit: |collector: &mut Collector| {
                        collector.add::<$args>();
                        collector.add::<$result>();
                        $(collector.add::<$stream>();)?
                    },
                },
            )+]
        }
    };
}

command_table! {
    // Workspace.
    /// Every project row and the default selection.
    "workspace" => Workspace(NoArgs) -> WorkspaceView = workspace::workspace;
    /// Adds a folder as a project and compiles it at once, or finds its row (main process only).
    "add_project" => AddProject(workspace::AddProjectArgs) -> AddedProject = workspace::add_project;
    /// Removes a project from the workspace (its `.unfold/` stays).
    "remove_project" => RemoveProject(ProjectArgs) -> () = workspace::remove_project;
    /// Expands or collapses a project's sidebar row.
    "set_project_expanded" => SetProjectExpanded(workspace::SetProjectExpandedArgs) -> () = workspace::set_project_expanded;
    /// The calling window's Home selection (keeps the project live).
    "select_project" => SelectProject(workspace::SelectProjectArgs) -> () = workspace::select_project;

    // Project, Home, settings.
    /// Home's header, state line, and Health line.
    "project_summary" => ProjectSummary(ProjectArgs) -> projects::ProjectSummary = projects::project_summary;
    /// The apps and entrances of `unfold.toml` as written (or one app, by its name or key),
    /// each `code` with its state and its fix when one exists.
    "entrances" => Entrances(entrances::EntrancesArgs) -> entrances::EntrancesView = entrances::entrances;
    /// The Problems list, with the health of every board.
    "problems" => Problems(ProjectArgs) -> Vec<ProblemItem> = compiling::problems;
    /// Compiles the project; streams progress.
    "compile" => Compile(ProjectArgs) -> CompileOutcome, stream CompileProgress = compiling::compile;
    /// Cancels a project's running and queued Compiles.
    "cancel_compile" => CancelCompile(ProjectArgs) -> compiling::CancelCompileResult = compiling::cancel_compile;
    /// A project's `unfold.toml`: its settings and apps.
    "project_config" => ProjectConfig(ProjectArgs) -> settings::ProjectConfig = settings::project_config;
    /// Applies validated edits to a project's `unfold.toml` and returns the diff. An edit of
    /// only `[[app]]` tables resolves the entrances again without compiling.
    "update_project_config" => UpdateProjectConfig(settings::UpdateProjectConfigArgs) -> settings::ProjectConfigUpdate = settings::update_project_config;
    /// The app settings.
    "get_settings" => GetSettings(NoArgs) -> AppSettings = settings::get_settings;
    /// Saves the app settings.
    "set_settings" => SetSettings(settings::SetSettingsArgs) -> AppSettings = settings::set_settings;
    /// The username that signs edits, and the engine version.
    "app_info" => AppInfo(NoArgs) -> settings::AppInfo = settings::app_info;

    // Files and graph.
    /// The code panel's model for one selection: a block, lines, a field, a file, or an app or
    /// entrance.
    "code_view" => CodeView(code::CodeViewArgs) -> unfold_reader::code_view::CodeView = code::code_view;
    /// Opens a file with the editor command, or in the default text editor. The project's `unfold.toml` is
    /// created empty first when it has none.
    "open_in_editor" => OpenInEditor(files::OpenInEditorArgs) -> () = files::open_in_editor;
    /// A card: a definition's drawn flow (with the page's overlay, the card's open paths, and
    /// Show all calls; with its agent text when asked), or an app's or entrance's card.
    "card" => Card(graph::CardArgs) -> graph::CardView = graph::card;
    /// Agent text: the compiled tree of a definition, with each block's label and key.
    "code_text" => CodeText(ProjectKeyArgs) -> unfold_reader::drawing::CodeText = graph::code_text;
    /// Callers (paginated), outgoing calls, and reached-from.
    "relations" => Relations(relations::RelationsArgs) -> relations::Relations = relations::relations;
    /// Show callers: the distinct definitions that call a definition.
    "callers" => Callers(relations::CallersArgs) -> relations::Callers = relations::callers;
    /// The pages of a project's boards where a key is a card ("Already on …").
    "board_refs" => BoardRefs(ProjectKeyArgs) -> boards::BoardRefs = boards::board_refs;

    // Contracts.
    /// A call's contract, by its block key: what it sends, what returns and where the value
    /// lands, and what the callee raises.
    "call_contract" => CallContract(ProjectKeyArgs) -> unfold_reader::contracts::CallContract = types::call_contract;
    /// A class or type: its members, aliased type, and use count.
    "type_definition" => TypeDefinition(ProjectKeyArgs) -> unfold_reader::contracts::TypeDefinition = types::type_definition;
    /// One page of the places a class or type is named.
    "type_uses" => TypeUses(types::TypeUsesArgs) -> unfold_reader::contracts::TypeUses = types::type_uses;

    // Search.
    /// Definitions, apps and entrances, and files by name, across the projects.
    "search" => Search(search::SearchArgs) -> search::SearchResults = search::search;

    // Boards.
    /// A project's boards, most recently edited first, and its empty folders.
    "list_boards" => ListBoards(ProjectArgs) -> boards::BoardList = boards::list_boards;
    /// A board as its file holds it, without opening it.
    "read_board" => ReadBoard(boards::BoardIdArgs) -> boards::ReadBoardResult = boards::read_board;
    /// Opens a board in the calling window.
    "open_board" => OpenBoard(boards::BoardIdArgs) -> boards::BoardState = boards::open_board;
    /// Creates an empty board file in `.unfold/boards/` and opens it in the calling window.
    "new_board" => NewBoard(boards::NewBoardArgs) -> boards::BoardState = boards::new_board;
    /// Reads an open board's file again.
    "reload_board" => ReloadBoard(boards::BoardIdArgs) -> boards::BoardState = boards::reload_board;
    /// Closes an open board.
    "close_board" => CloseBoard(boards::BoardIdArgs) -> () = boards::close_board;
    /// Validates, signs, and writes an edited board over its file.
    "save_board" => SaveBoard(boards::SaveBoardArgs) -> boards::BoardState = boards::save_board;
    /// An open board's health.
    "board_health" => BoardHealth(boards::BoardIdArgs) -> boards::BoardHealthView = boards::board_health;
    /// Applies one repair to an open board and saves it.
    "repair_board" => RepairBoard(boards::RepairBoardArgs) -> boards::BoardState = boards::repair_board;
    /// Deletes a board.
    "delete_board" => DeleteBoard(boards::BoardIdArgs) -> () = boards::delete_board;
    /// Moves any board into a folder.
    "set_board_folder" => SetBoardFolder(boards::SetBoardFolderArgs) -> boards::BoardSummary = boards::set_board_folder;
    /// Adds an empty board folder.
    "create_folder" => CreateFolder(ProjectPathArgs) -> () = boards::create_folder;
    /// Renames a board folder and moves its boards.
    "rename_folder" => RenameFolder(boards::RenameFolderArgs) -> boards::RenamedFolder = boards::rename_folder;
    /// Deletes an empty board folder.
    "delete_folder" => DeleteFolder(ProjectPathArgs) -> () = boards::delete_folder;

    // The window.
    /// The remembered window: bounds, whether it was maximized, the last board shown, and the Show all
    /// calls switch (main process only).
    "window_state" => WindowState(NoArgs) -> Option<WindowState> = windows::window_state;
    /// Remembers the window's bounds, whether it is maximized, the board it shows, and the Show all
    /// calls switch (main process only).
    "save_window_state" => SaveWindowState(windows::SaveWindowStateArgs) -> () = windows::save_window_state;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_snake_case() {
        let mut names = Request::NAMES.to_vec();
        assert!(
            names
                .iter()
                .all(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        );
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Request::NAMES.len());
    }

    #[cfg(feature = "ts")]
    #[test]
    fn the_specs_follow_the_table() {
        let specs: Vec<&str> = specs().iter().map(|spec| spec.name).collect();
        assert_eq!(specs, Request::NAMES);
    }

    #[test]
    fn requests_deserialize_by_name() {
        let request: Request = serde_json::from_str(
            r#"{"command":"create_folder","args":{"project":"p-aaaaaaaaaa","path":"src"}}"#,
        )
        .expect("parse");
        assert_eq!(request.name(), "create_folder");
        let request: Request =
            serde_json::from_str(r#"{"command":"compile","args":{"project":"p-aaaaaaaaaa"}}"#)
                .expect("parse");
        assert!(matches!(request, Request::Compile(_)));
        assert!(
            serde_json::from_str::<Request>(
                r#"{"command":"compile","args":{"project":"p-aaaaaaaaaa","mode":"check"}}"#
            )
            .is_err(),
            "a compile has no mode"
        );
    }
}
