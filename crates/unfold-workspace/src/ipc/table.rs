//! The command table: every command's name, argument type, result type, stream message type,
//! and handler, in one list (architecture §12, decision U-25).
//!
//! [`Request`] is the serde-tagged request (`{"command": "<name>", "args": {…}}`) the addon's
//! `dispatch` receives; [`Request::run`] calls the handler and serializes its result. The same
//! list feeds [`specs`], from which `cargo xtask gen-ipc` writes the UI's `Commands` map.
//!
//! **Adding a command** (see `docs/ipc.md`): write the handler and its `ts_rs::TS` argument and
//! result types in a `commands` module, add one line here, and run `cargo xtask gen-ipc`.

use serde::Deserialize;

use crate::commands::{
    KeyArgs, NoArgs, PathArgs, boards, compiling, diagnostics, entrances, export, file_view, files, folders,
    graph, relations, search, settings, text_search, windows,
};
use crate::compile::{CompileOutcome, CompileProgress};
use crate::ipc::context::Ctx;
use crate::ipc::error::CommandResult;
use crate::ipc::typescript::{Collector, CommandSpec, ts_name};
use crate::problems::ProblemItem;
use crate::state::{Preferences, RecentState, WindowState};
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
    // Folders.
    /// What a Compile would graph and skip, for the first-Compile screen. Writes nothing.
    "folder_summary" => FolderSummary(NoArgs) -> folders::FolderSummary = folders::folder_summary;
    /// The recent folders and the welcome window's bounds.
    "recent_folders" => RecentFolders(NoArgs) -> RecentState = folders::recent_folders;
    /// Removes a folder from the recent list.
    "forget_recent_folder" => ForgetRecentFolder(folders::ForgetRecentArgs) -> RecentState = folders::forget_recent_folder;

    // Compile.
    /// Compile (`check`) or Rebuild from scratch (`from_scratch`); streams progress.
    "compile" => Compile(compiling::CompileArgs) -> CompileOutcome, stream CompileProgress = compiling::compile;
    /// Cancels the running and queued Compiles.
    "cancel_compile" => CancelCompile(NoArgs) -> compiling::CancelCompileResult = compiling::cancel_compile;
    /// Turns background updates on or off for the folder.
    "set_background_updates" => SetBackgroundUpdates(compiling::SetBackgroundUpdatesArgs) -> GraphStatus = compiling::set_background_updates;
    /// The status bar's state.
    "graph_status" => GraphStatus(NoArgs) -> GraphStatus = compiling::graph_status;
    /// The Problems list.
    "problems" => Problems(NoArgs) -> Vec<ProblemItem> = compiling::problems;
    /// The Index log and the graph's counts.
    "index_log" => IndexLog(NoArgs) -> compiling::IndexLog = compiling::index_log;

    // Files.
    /// One level of the folder tree with its marks.
    "list_dir" => ListDir(PathArgs) -> files::DirListing = files::list_dir;
    /// A source tab's render model, or one page of lines.
    "file_view" => FileView(file_view::FileViewArgs) -> file_view::FileView = file_view::file_view;
    /// A file's definitions, classes holding their methods.
    "file_outline" => FileOutline(PathArgs) -> files::FileOutline = files::file_outline;
    /// Opens a file in the preferred editor, or as text in the system's text editor.
    "open_in_editor" => OpenInEditor(files::OpenInEditorArgs) -> files::OpenedWith = files::open_in_editor;
    /// Shows a file or folder in the system's file manager.
    "reveal_in_finder" => RevealInFinder(PathArgs) -> () = files::reveal_in_finder;

    // Graph.
    /// An entrance card or a block card (Code and Flow).
    "card" => Card(graph::CardArgs) -> graph::CardView = graph::card;
    /// The hover over a call link: signature line and location.
    "peek" => Peek(KeyArgs) -> graph::Peek = graph::peek;
    /// Callers (paginated), outgoing calls grouped, and reached-from.
    "relations" => Relations(relations::RelationsArgs) -> relations::Relations = relations::relations;
    /// The Entrances view: applications, launches, entrances.
    "entrance_tree" => EntranceTree(NoArgs) -> entrances::EntranceTree = entrances::entrance_tree;

    // Search.
    /// Definitions, entrances, and files by name (also ⌘P and ⌘T).
    "search" => Search(search::SearchArgs) -> search::SearchResults = search::search;
    /// Text matches across the folder; streams one message per file.
    "search_text" => SearchText(text_search::TextSearchArgs) -> text_search::TextSearchSummary, stream text_search::TextSearchFile = text_search::search_text;

    // Boards.
    /// Published boards and drafts.
    "list_boards" => ListBoards(NoArgs) -> boards::BoardList = boards::list_boards;
    /// Opens a board in the calling window, or names the window that has it.
    "open_board" => OpenBoard(boards::BoardIdArgs) -> boards::OpenBoardResult = boards::open_board;
    /// Reads an open board from disk again.
    "reload_board" => ReloadBoard(boards::BoardIdArgs) -> boards::BoardState = boards::reload_board;
    /// Creates and opens an empty draft.
    "new_board" => NewBoard(boards::NewBoardArgs) -> boards::BoardState = boards::new_board;
    /// Closes a board in the calling window.
    "close_board" => CloseBoard(boards::BoardIdArgs) -> () = boards::close_board;
    /// Moves a board to another window of the folder.
    "move_board" => MoveBoard(boards::MoveBoardArgs) -> () = boards::move_board;
    /// Validates, signs, and writes an edited board (autosave).
    "save_board" => SaveBoard(boards::SaveBoardArgs) -> boards::BoardState = boards::save_board;
    /// Publishes a draft to `.unfold/boards/`.
    "publish_board" => PublishBoard(boards::PublishBoardArgs) -> boards::BoardWriteOutcome = boards::publish_board;
    /// Moves a published board back to drafts.
    "unpublish_board" => UnpublishBoard(boards::BoardIdArgs) -> boards::BoardState = boards::unpublish_board;
    /// Renames a board's title, or a published board's file.
    "rename_board" => RenameBoard(boards::RenameBoardArgs) -> boards::BoardWriteOutcome = boards::rename_board;
    /// Deletes a board.
    "delete_board" => DeleteBoard(boards::BoardIdArgs) -> () = boards::delete_board;
    /// A board's health and outline.
    "board_health" => BoardHealth(boards::BoardIdArgs) -> boards::BoardHealthView = boards::board_health;
    /// Applies one health repair and saves.
    "repair_board" => RepairBoard(boards::RepairBoardArgs) -> boards::BoardState = boards::repair_board;

    // Export.
    /// Writes Mermaid to a path chosen in a save dialog (main process only).
    "export_mermaid" => ExportMermaid(export::ExportMermaidArgs) -> export::ExportedFile = export::export_mermaid;
    /// Mermaid text for the clipboard.
    "export_mermaid_text" => ExportMermaidText(export::MermaidTextArgs) -> export::MermaidText = export::export_mermaid_text;

    // Settings.
    /// The app preferences.
    "get_preferences" => GetPreferences(NoArgs) -> Preferences = settings::get_preferences;
    /// Saves the app preferences.
    "set_preferences" => SetPreferences(settings::SetPreferencesArgs) -> Preferences = settings::set_preferences;
    /// The folder's `.unfold/config.toml`.
    "get_folder_config" => GetFolderConfig(NoArgs) -> settings::FolderConfig = settings::get_folder_config;
    /// Applies validated edits to `.unfold/config.toml`.
    "update_folder_config" => UpdateFolderConfig(settings::UpdateFolderConfigArgs) -> settings::FolderConfig = settings::update_folder_config;
    /// The username that signs edits, and the engine version.
    "app_info" => AppInfo(NoArgs) -> settings::AppInfo = settings::app_info;
    /// A redacted summary for bug reports (Copy Diagnostics): versions, platform, counts; no
    /// source and no paths.
    "diagnostics" => Diagnostics(NoArgs) -> diagnostics::DiagnosticsReport = diagnostics::diagnostics;

    // Windows.
    /// What a window remembered (tabs, layout, open boards).
    "window_state" => WindowState(windows::WindowKeyArgs) -> Option<WindowState> = windows::window_state;
    /// Remembers the calling window's tabs, layout, and open boards.
    "save_window_state" => SaveWindowState(windows::SaveWindowStateArgs) -> () = windows::save_window_state;
    /// Forgets a closed window.
    "forget_window_state" => ForgetWindowState(windows::WindowKeyArgs) -> () = windows::forget_window_state;
    /// A folder's saved windows, for restoring them (main process only).
    "folder_windows" => FolderWindows(windows::FolderWindowsArgs) -> windows::FolderWindows = windows::folder_windows;
    /// Remembers a window's position and size.
    "save_window_bounds" => SaveWindowBounds(windows::WindowBoundsArgs) -> () = windows::save_window_bounds;
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
            serde_json::from_str(r#"{"command":"list_dir","args":{"path":"src"}}"#).expect("parse");
        assert_eq!(request.name(), "list_dir");
        let request: Request =
            serde_json::from_str(r#"{"command":"compile","args":{"mode":"from_scratch"}}"#).expect("parse");
        assert!(matches!(request, Request::Compile(_)));
    }
}
