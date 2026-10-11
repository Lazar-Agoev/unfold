//! The command table: every command's name, argument type, result type, stream message type,
//! and handler, in one list (docs/ipc.md#how-a-command-travels, generated command types).
//!
//! [`Request`] is the serde-tagged request (`{"command": "<name>", "args": {…}}`) the addon's
//! `dispatch` receives; [`Request::run`] calls the handler and serializes its result. The same
//! list feeds [`specs`], from which `cargo xtask gen-ipc` writes the UI's `Commands` map.
//!
//! **Adding a command** (see `docs/ipc.md`): write the handler and its `ts_rs::TS` argument and
//! result types in a `commands` module, add one line here, and run `cargo xtask gen-ipc`.

use serde::Deserialize;

use crate::commands::{
    NoArgs, ProjectArgs, ProjectKeyArgs, ProjectPathArgs, boards, compiling, diagnostics, entrances, export,
    file_view, files, graph, projects, relations, search, settings, text_search, types, windows, workspace,
};
use crate::compile::{CompileOutcome, CompileProgress};
use crate::git::GitInfo;
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::ipc::typescript::{Collector, CommandSpec, ts_name};
use crate::problems::ProblemItem;
use crate::projects::{AddedProject, ProjectRow, WorkspaceView};
use crate::state::{Preferences, SavedWindow};
use crate::status::GraphStatus;

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

            /// Runs the command's handler and serializes its result.
            pub(crate) fn run(self, ctx: &Ctx<'_>) -> CommandResult<String> {
                match self {
                    $(Self::$variant(args) => crate::host::encode(&$handler(ctx, args)?),)+
                }
            }
        }

        /// The command table for TypeScript generation, in table order.
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
    /// Every project row, the default selection, recent boards, and the folders "Opened before".
    "workspace" => Workspace(NoArgs) -> WorkspaceView = workspace::workspace;
    /// Adds a folder as a project, or finds its row (main process only).
    "add_project" => AddProject(workspace::AddProjectArgs) -> AddedProject = workspace::add_project;
    /// Adds back a folder "Opened before" lists.
    "readd_project" => ReaddProject(workspace::AddProjectArgs) -> AddedProject = workspace::readd_project;
    /// Removes a project from the workspace (its `.unfold/` stays).
    "remove_project" => RemoveProject(ProjectArgs) -> () = workspace::remove_project;
    /// Puts the project rows in a new order.
    "reorder_projects" => ReorderProjects(workspace::ReorderProjectsArgs) -> WorkspaceView = workspace::reorder_projects;
    /// Expands or collapses a project's sidebar row.
    "set_project_expanded" => SetProjectExpanded(workspace::SetProjectExpandedArgs) -> () = workspace::set_project_expanded;
    /// The calling window's Home selection (keeps the project live).
    "select_project" => SelectProject(workspace::SelectProjectArgs) -> () = workspace::select_project;
    /// The branch and worktrees of a project's repository.
    "git_info" => GitInfo(ProjectArgs) -> Option<GitInfo> = workspace::git_info;
    /// Points a project at another worktree of its repository.
    "switch_worktree" => SwitchWorktree(workspace::SwitchWorktreeArgs) -> ProjectRow = workspace::switch_worktree;
    /// Every project's drafts and published boards with unpublished changes.
    "drafts" => Drafts(NoArgs) -> boards::Drafts = boards::drafts;

    // Project, Home, settings.
    /// Home's header and Health line.
    "project_summary" => ProjectSummary(ProjectArgs) -> projects::ProjectSummary = projects::project_summary;
    /// What a Compile would graph and skip. Writes nothing.
    "compile_preview" => CompilePreview(ProjectArgs) -> projects::CompilePreview = projects::compile_preview;
    /// One level of the scope tree: folders only.
    "scope_dir" => ScopeDir(ProjectPathArgs) -> projects::ScopeDir = projects::scope_dir;
    /// How a project starts: applications, launches, and entrances, paged and filtered.
    "entrances" => Entrances(entrances::EntrancesArgs) -> entrances::EntranceMap = entrances::entrances;
    /// The Problems list, with the health of every board.
    "problems" => Problems(ProjectArgs) -> Vec<ProblemItem> = compiling::problems;
    /// The Index log and the graph's counts.
    "index_log" => IndexLog(ProjectArgs) -> compiling::IndexLog = compiling::index_log;
    /// Compile (`check`) or Rebuild from scratch (`from_scratch`); streams progress.
    "compile" => Compile(compiling::CompileArgs) -> CompileOutcome, stream CompileProgress = compiling::compile;
    /// Cancels a project's running and queued Compiles.
    "cancel_compile" => CancelCompile(ProjectArgs) -> compiling::CancelCompileResult = compiling::cancel_compile;
    /// Turns background updates on or off for a project.
    "set_background_updates" => SetBackgroundUpdates(compiling::SetBackgroundUpdatesArgs) -> GraphStatus = compiling::set_background_updates;
    /// A project's `unfold.toml` and shared name.
    "project_config" => ProjectConfig(ProjectArgs) -> settings::ProjectConfig = settings::project_config;
    /// Applies validated edits to a project's `unfold.toml`.
    "update_project_config" => UpdateProjectConfig(settings::UpdateProjectConfigArgs) -> settings::ProjectConfig = settings::update_project_config;
    /// The app preferences.
    "get_preferences" => GetPreferences(NoArgs) -> Preferences = settings::get_preferences;
    /// Saves the app preferences.
    "set_preferences" => SetPreferences(settings::SetPreferencesArgs) -> Preferences = settings::set_preferences;
    /// The username that signs edits, and the engine version.
    "app_info" => AppInfo(NoArgs) -> settings::AppInfo = settings::app_info;
    /// A redacted summary for bug reports (Copy Diagnostics): versions, platform, counts; no
    /// source and no paths.
    "diagnostics" => Diagnostics(diagnostics::DiagnosticsArgs) -> diagnostics::DiagnosticsReport = diagnostics::diagnostics;

    // Files and graph.
    /// The peek sheet's render model of a file, or one page of lines.
    "file_view" => FileView(file_view::FileViewArgs) -> file_view::FileView = file_view::file_view;
    /// Opens a file in the preferred editor, or as text in the system's text editor.
    "open_in_editor" => OpenInEditor(files::OpenInEditorArgs) -> files::OpenedWith = files::open_in_editor;
    /// Shows a file or folder in the system's file manager.
    "reveal_in_finder" => RevealInFinder(ProjectPathArgs) -> () = files::reveal_in_finder;
    /// An entrance card or a block card (Code and Flow).
    "card" => Card(graph::CardArgs) -> graph::CardView = graph::card;
    /// The hover over a call link: signature line and location.
    "peek" => Peek(ProjectKeyArgs) -> graph::Peek = graph::peek;
    /// Callers (paginated), outgoing calls grouped, and reached-from.
    "relations" => Relations(relations::RelationsArgs) -> relations::Relations = relations::relations;
    /// Show callers: the distinct definitions that call a block.
    "callers" => Callers(relations::CallersArgs) -> relations::Callers = relations::callers;
    /// Add's preview pane and the map popover.
    "preview" => Preview(ProjectKeyArgs) -> graph::Preview = graph::preview;
    /// The board pages of every project where a key is a card ("Already on …").
    "board_refs" => BoardRefs(ProjectKeyArgs) -> boards::BoardRefs = boards::board_refs;

    // Typed lines.
    /// The at-rest labels of a page's arrows: what goes in and what comes back.
    "line_labels" => LineLabels(types::LineLabelsArgs) -> Vec<types::LabeledLine> = types::line_labels;
    /// The contract of one selected line: sends, returns, throws, and the lines it read.
    "call_contract" => CallContract(types::CallContractArgs) -> unfold_reader::contracts::CallContract = types::call_contract;
    /// A type declaration or class: its members, aliased type, and use count.
    "type_definition" => TypeDefinition(ProjectKeyArgs) -> unfold_reader::contracts::TypeDefinition = types::type_definition;
    /// One page of the places a type declaration or class is written.
    "type_uses" => TypeUses(types::TypeUsesArgs) -> unfold_reader::contracts::TypeUses = types::type_uses;

    // Search.
    /// Definitions, entrances, and files by name, across the projects.
    "search" => Search(search::SearchArgs) -> search::SearchResults = search::search;
    /// Text matches across the projects; streams one message per file.
    "search_text" => SearchText(text_search::TextSearchArgs) -> text_search::TextSearchSummary, stream text_search::TextSearchFile = text_search::search_text;

    // Boards.
    /// A project's boards and empty folders.
    "list_boards" => ListBoards(ProjectArgs) -> boards::BoardList = boards::list_boards;
    /// What the thumbnails of a project's boards draw.
    "board_thumbnails" => BoardThumbnails(boards::ThumbnailsArgs) -> boards::Thumbnails = boards::board_thumbnails;
    /// A board as its working file holds it, without opening it.
    "read_board" => ReadBoard(boards::BoardIdArgs) -> boards::ReadBoardResult = boards::read_board;
    /// Opens a board in the calling window, or names the window that has it.
    "open_board" => OpenBoard(boards::BoardIdArgs) -> boards::OpenBoardResult = boards::open_board;
    /// Creates an empty draft and opens it in the calling window.
    "new_board" => NewBoard(boards::NewBoardArgs) -> boards::BoardState = boards::new_board;
    /// Reads an open board's working file again.
    "reload_board" => ReloadBoard(boards::BoardIdArgs) -> boards::BoardState = boards::reload_board;
    /// Closes a board in the calling window.
    "close_board" => CloseBoard(boards::BoardIdArgs) -> () = boards::close_board;
    /// Moves an open board to another window.
    "move_board" => MoveBoard(boards::MoveBoardArgs) -> () = boards::move_board;
    /// Validates, signs, and writes an edited board.
    "save_board" => SaveBoard(boards::SaveBoardArgs) -> boards::BoardState = boards::save_board;
    /// Remembers a board's active page and each page's camera and lens.
    "save_board_view" => SaveBoardView(boards::SaveBoardViewArgs) -> () = boards::save_board_view;
    /// An open board's health and outlines.
    "board_health" => BoardHealth(boards::BoardIdArgs) -> boards::BoardHealthView = boards::board_health;
    /// Applies one repair to an open board and saves it.
    "repair_board" => RepairBoard(boards::RepairBoardArgs) -> boards::BoardState = boards::repair_board;
    /// Publishes a draft, or a published board's unpublished changes.
    "publish_board" => PublishBoard(boards::PublishBoardArgs) -> boards::PublishOutcome = boards::publish_board;
    /// Drops a published board's unpublished changes.
    "discard_changes" => DiscardChanges(boards::BoardIdArgs) -> boards::BoardState = boards::discard_changes;
    /// Moves a published board back to drafts.
    "unpublish_board" => UnpublishBoard(boards::BoardIdArgs) -> boards::BoardState = boards::unpublish_board;
    /// Deletes a board.
    "delete_board" => DeleteBoard(boards::BoardIdArgs) -> () = boards::delete_board;
    /// Copies a board into a new draft.
    "duplicate_board" => DuplicateBoard(boards::BoardIdArgs) -> boards::BoardSummary = boards::duplicate_board;
    /// Moves any board into a folder.
    "set_board_folder" => SetBoardFolder(boards::SetBoardFolderArgs) -> boards::BoardSummary = boards::set_board_folder;
    /// Adds an empty board folder.
    "create_folder" => CreateFolder(ProjectPathArgs) -> () = boards::create_folder;
    /// Renames a board folder and moves its boards.
    "rename_folder" => RenameFolder(boards::RenameFolderArgs) -> boards::RenamedFolder = boards::rename_folder;
    /// Deletes an empty board folder.
    "delete_folder" => DeleteFolder(ProjectPathArgs) -> () = boards::delete_folder;

    // Export.
    /// Writes a board page or an entrance as Mermaid to a chosen file (main process only).
    "export_mermaid" => ExportMermaid(export::ExportMermaidArgs) -> export::ExportedFile = export::export_mermaid;
    /// A board page or an entrance as Mermaid text.
    "export_mermaid_text" => ExportMermaidText(export::MermaidTextArgs) -> export::MermaidText = export::export_mermaid_text;

    // Windows.
    /// What a window remembered (Home selection, tabs, layout, bounds).
    "window_state" => WindowState(windows::WindowKeyArgs) -> Option<SavedWindow> = windows::window_state;
    /// Remembers the calling window's Home selection, tabs, and layout.
    "save_window_state" => SaveWindowState(windows::SaveWindowStateArgs) -> () = windows::save_window_state;
    /// Every remembered window, for restoring them at launch (main process only).
    "saved_windows" => SavedWindows(NoArgs) -> windows::SavedWindows = windows::saved_windows;
    /// Remembers a window's position and size (main process only).
    "save_window_bounds" => SaveWindowBounds(windows::WindowBoundsArgs) -> () = windows::save_window_bounds;
    /// Forgets a closed window (main process only).
    "forget_window" => ForgetWindow(windows::WindowKeyArgs) -> () = windows::forget_window;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_snake_case_and_match_the_specs() {
        let mut names = Request::NAMES.to_vec();
        assert!(
            names
                .iter()
                .all(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        );
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Request::NAMES.len());
        let specs: Vec<&str> = specs().iter().map(|spec| spec.name).collect();
        assert_eq!(specs, Request::NAMES);
    }

    #[test]
    fn requests_deserialize_by_name() {
        let request: Request =
            serde_json::from_str(r#"{"command":"scope_dir","args":{"project":"p-aaaaaaaaaa","path":"src"}}"#)
                .expect("parse");
        assert_eq!(request.name(), "scope_dir");
        let request: Request = serde_json::from_str(
            r#"{"command":"compile","args":{"project":"p-aaaaaaaaaa","mode":"from_scratch"}}"#,
        )
        .expect("parse");
        assert!(matches!(request, Request::Compile(_)));
    }
}
