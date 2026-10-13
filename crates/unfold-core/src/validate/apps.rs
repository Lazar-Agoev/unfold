//! Apps and entrances: keys re-derived from their names and unique in the graph, and every
//! entrance starting somewhere. A target's state is data, not an error, so states are not
//! checked: a `missing` target is valid.

use rustc_hash::FxHashSet;

use super::{Checker, Subject};
use crate::key::{Key, SlugError};

pub(super) fn check(checker: &mut Checker<'_>) {
    let graph = checker.context.graph;
    let mut apps: FxHashSet<&str> = FxHashSet::default();
    let mut entrances: FxHashSet<&str> = FxHashSet::default();
    for app in &graph.apps {
        let subject = || Subject::App(app.key.clone());
        check_key(checker, &subject, &app.key, Key::app(&app.name));
        if !apps.insert(app.key.as_str()) {
            checker.report(subject(), "another app has this key");
        }
        for entrance in &app.entrances {
            let subject = || Subject::Entrance(entrance.key.clone());
            check_key(
                checker,
                &subject,
                &entrance.key,
                Key::entrance(&app.name, &entrance.name),
            );
            if !entrances.insert(entrance.key.as_str()) {
                checker.report(subject(), "another entrance has this key");
            }
            if entrance.code.is_empty() {
                checker.report(subject(), "an entrance starts at one or more code keys");
            }
        }
    }
}

fn check_key(
    checker: &mut Checker<'_>,
    subject: &dyn Fn() -> Subject,
    key: &Key,
    derived: Result<Key, SlugError>,
) {
    match derived {
        Ok(derived) if derived != *key => {
            checker.report(subject(), format!("key differs from its derivation `{derived}`"));
        }
        Ok(_) => {}
        Err(error) => checker.report(subject(), error.to_string()),
    }
}
