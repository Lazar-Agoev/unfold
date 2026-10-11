//! How the entrance catalog describes a launch (docs/reader.md#derived-views): whether Unfold
//! can follow it, where it runs, what it starts, and its schedule in words.

use unfold_core::vocab::{Language, Reason, Resolution};
use unfold_core::{Entrance, Launch, LaunchMode};

/// Whether Unfold can follow a launch into the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaunchState {
    /// It starts indexed code.
    Ok,
    /// Its command can't be read, or what it names can't be resolved.
    Unreadable,
    /// It starts code that isn't indexed.
    NotIndexed,
    /// What it starts doesn't exist.
    NotFound,
}

/// The state of a launch (a process entrance) with its record.
pub fn launch_state(entrance: &Entrance, launch: Option<&Launch>) -> LaunchState {
    let Some(start) = entrance.targets.first() else {
        return LaunchState::Unreadable;
    };
    if start.resolution == Resolution::Resolved {
        return LaunchState::Ok;
    }
    if launch.is_some_and(|launch| launch.problem.is_some()) {
        return LaunchState::Unreadable;
    }
    match start.reason {
        Some(Reason::NotIndexed) => LaunchState::NotIndexed,
        Some(Reason::NotFound) => LaunchState::NotFound,
        _ => LaunchState::Unreadable,
    }
}

/// Whether a launch puts its application under "Needs a look": its command can't be read and
/// its target isn't set by hand.
pub fn needs_look(launch: &Launch) -> bool {
    launch.problem.is_some() && !launch.target_set_by_hand
}

/// The folder a launch runs in, as the catalog shows it: the project's name for its folder.
pub fn folder_label(folder: &str, project: &str) -> String {
    if folder == "." {
        project.to_owned()
    } else {
        folder.to_owned()
    }
}

/// What a launch starts, in words: `FastAPI application` when routes are bound to its target,
/// else by how it starts and the language of what it starts.
pub fn start_kind(mode: Option<LaunchMode>, language: Language, has_routes: bool) -> &'static str {
    if has_routes {
        return "FastAPI application";
    }
    match (mode, language) {
        (Some(LaunchMode::PythonModule), _) => "Python module",
        (Some(LaunchMode::PythonCallable), _) => "Python function",
        (Some(LaunchMode::PythonAsgi), _) => "ASGI application",
        (_, Language::Python) => "Python script",
        (_, Language::TypeScript) => "Node file",
    }
}

/// A plain reading of a standard five-field cron schedule: `30 7 * * *` is `daily at 07:30`,
/// `0 5,11,17,23 * * *` is `4 times a day`. Anything else has none.
pub fn schedule_gloss(schedule: &str) -> Option<String> {
    let fields: Vec<&str> = schedule.split_whitespace().collect();
    let [minute, hours, "*", "*", "*"] = fields.as_slice() else {
        return None;
    };
    let number = |text: &str, max: u32| text.parse::<u32>().ok().filter(|&value| value <= max);
    let minute = number(minute, 59)?;
    let hours: Vec<u32> = hours
        .split(',')
        .map(|hour| number(hour, 23))
        .collect::<Option<_>>()?;
    match hours.as_slice() {
        [hour] => Some(format!("daily at {hour:02}:{minute:02}")),
        many => Some(format!("{} times a day", many.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_crons_have_a_gloss() {
        assert_eq!(schedule_gloss("30 7 * * *").as_deref(), Some("daily at 07:30"));
        assert_eq!(
            schedule_gloss("0 5,11,17,23 * * *").as_deref(),
            Some("4 times a day")
        );
        assert_eq!(schedule_gloss(" 5  0 * * * ").as_deref(), Some("daily at 00:05"));
        for other in [
            "*/5 * * * *",
            "0 7 * * 1",
            "60 7 * * *",
            "0 24 * * *",
            "@daily",
            "0 7 * *",
            "0 1-3 * * *",
        ] {
            assert_eq!(schedule_gloss(other), None, "{other}");
        }
    }

    #[test]
    fn the_project_folder_shows_the_project_name() {
        assert_eq!(folder_label(".", "orders"), "orders");
        assert_eq!(folder_label("data-pipelines", "orders"), "data-pipelines");
    }
}
