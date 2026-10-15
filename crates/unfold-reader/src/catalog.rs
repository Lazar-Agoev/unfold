//! The catalog: the definitions that can be cards, with their labels and qualified names, and
//! the apps and entrances of `unfold.toml` found by key.
//!
//! - **Definitions** are every module, class, function (inline functions included), and type.
//!   [`definitions`] lists one file's for the store, which indexes them for search; it needs only
//!   that file, so a commit recomputes the files it changes.
//! - A definition's **label** is its name. A module's is its path. An inline function or class
//!   written in a call's arguments is labeled by the property it is written as when it is one
//!   (`onSuccess` in `useMutation({ onSuccess: () => … })`), and is otherwise a
//!   `callback to <callee>`, after that call's written callee; any other inline function is an
//!   `anonymous function`, and any other inline class an `anonymous class`.
//! - The **qualified name** puts the names of the enclosing named definitions before the label,
//!   joined with `.` (`OrderService.place`). A module's is its path.
//! - An app's start and its entrances are [`Start`]s, found by key with [`start`]. A start's
//!   title is its name.

use unfold_core::vocab::NodeKind;
use unfold_core::{App, Entrance, EntranceTarget, FileGraph, Key, Node, RepoPath};
use unfold_store::Definition;

use crate::tree::to_u32;

/// Node `id`'s label: a definition's name, a module's path, the property a callback is written
/// as, `callback to <callee>` for any other inline function or class written in a call's
/// arguments, `anonymous function` or `anonymous class` for any other. Any other node is labeled
/// by its name (a call's callee) or its form.
pub fn label(nodes: &[Node], path: &RepoPath, id: u32) -> String {
    let node = &nodes[id as usize];
    if let Some(name) = &node.name {
        return name.to_string();
    }
    match node.kind() {
        NodeKind::Module => path.as_str().to_owned(),
        NodeKind::Function | NodeKind::Class => {
            let call = node
                .parent
                .map(|parent| &nodes[parent as usize])
                .filter(|parent| parent.kind() == NodeKind::Call)
                // A function called where it is written is the call's callee, not its argument.
                .filter(|call| !call.head.is_some_and(|head| head.contains(node.range)));
            match call.and_then(|call| call.name.as_deref()) {
                Some(callee) => format!("callback to {callee}"),
                None => format!("anonymous {}", node.kind()),
            }
        }
        _ => node.form.as_str().to_owned(),
    }
}

/// Node `id`'s qualified name: the names of the named definitions it is written in, outermost
/// first, then its label, joined with `.`. A module's is its path.
pub fn qualified_name(nodes: &[Node], path: &RepoPath, id: u32) -> String {
    let mut names: Vec<&str> = Vec::new();
    let mut parent = nodes[id as usize].parent;
    while let Some(ancestor) = parent {
        let node = &nodes[ancestor as usize];
        if node.kind().is_definition()
            && let Some(name) = &node.name
        {
            names.push(name);
        }
        parent = node.parent;
    }
    let mut qualified = String::new();
    for name in names.iter().rev() {
        qualified.push_str(name);
        qualified.push('.');
    }
    qualified.push_str(&label(nodes, path, id));
    qualified
}

/// The catalog entries of one file: every definition, in preorder.
pub fn definitions(file: &FileGraph) -> Vec<Definition> {
    let path = &file.file.path;
    file.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind().is_definition())
        .map(|(id, node)| {
            let id = to_u32(id);
            Definition {
                key: node.key.clone(),
                node: id,
                kind: node.kind(),
                qualified_name: qualified_name(&file.nodes, path, id).into(),
                label: label(&file.nodes, path, id).into(),
            }
        })
        .collect()
}

/// An app's start (`entrance:<app>`) or one of its entrances (`entrance:<app>/<entrance>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start<'a> {
    /// The app itself: what its `run` command starts.
    App(&'a App),
    /// One entrance of an app.
    Entrance {
        /// The app it belongs to.
        app: &'a App,
        /// The entrance.
        entrance: &'a Entrance,
    },
}

impl<'a> Start<'a> {
    /// The start's key.
    pub fn key(&self) -> &'a Key {
        match self {
            Self::App(app) => &app.key,
            Self::Entrance { entrance, .. } => &entrance.key,
        }
    }

    /// Its name, which is also its title.
    pub fn name(&self) -> &'a str {
        match self {
            Self::App(app) => &app.name,
            Self::Entrance { entrance, .. } => &entrance.name,
        }
    }

    /// What starts it, as written: an app's `run` command, an entrance's `on`.
    pub fn on(&self) -> Option<&'a str> {
        match self {
            Self::App(app) => app.run.as_deref(),
            Self::Entrance { entrance, .. } => entrance.on.as_deref(),
        }
    }

    /// Its one sentence, if written.
    pub fn about(&self) -> Option<&'a str> {
        match self {
            Self::App(app) => app.about.as_deref(),
            Self::Entrance { entrance, .. } => entrance.about.as_deref(),
        }
    }

    /// Its `code` keys in order, each with its state; empty for an app without `code`.
    pub fn targets(&self) -> &'a [EntranceTarget] {
        match self {
            Self::App(app) => app.code.as_slice(),
            Self::Entrance { entrance, .. } => &entrance.code,
        }
    }

    /// The app it is, or belongs to.
    pub fn app(&self) -> &'a App {
        match self {
            Self::App(app) | Self::Entrance { app, .. } => app,
        }
    }
}

/// Every start of `apps`: each app, followed by its entrances, in file order.
pub fn starts(apps: &[App]) -> impl Iterator<Item = Start<'_>> {
    apps.iter().flat_map(|app| {
        std::iter::once(Start::App(app)).chain(
            app.entrances
                .iter()
                .map(move |entrance| Start::Entrance { app, entrance }),
        )
    })
}

/// The app or entrance of `apps` with `key`.
pub fn start<'a>(apps: &'a [App], key: &str) -> Option<Start<'a>> {
    starts(apps).find(|start| start.key().as_str() == key)
}

#[cfg(test)]
mod tests {
    use unfold_core::testkit::sample;

    use super::*;

    #[test]
    fn labels_and_qualified_names_follow_the_tree() {
        let graph = sample::shop().build();
        let orders = graph.file("src/orders.ts").unwrap();
        let entries = definitions(orders);
        let labels: Vec<(&str, &str, &str)> = entries
            .iter()
            .map(|entry| (entry.key.as_str(), &*entry.label, &*entry.qualified_name))
            .collect();
        assert_eq!(
            labels,
            [
                ("src/orders.ts#", "src/orders.ts", "src/orders.ts"),
                ("src/orders.ts#Order", "Order", "Order"),
                (
                    "src/orders.ts#/call:ordersRouter.post(\"/orders/:id\")/callback[0]",
                    "callback to ordersRouter.post",
                    "callback to ordersRouter.post"
                ),
                ("src/orders.ts#loadOrder", "loadOrder", "loadOrder"),
            ]
        );
        assert!(entries.iter().all(|entry| entry.kind.is_definition()));

        let payments = graph.file("shop/payments.py").unwrap();
        let lambda = payments
            .local_id("shop/payments.py#retry/return[0]/lambda[0]")
            .unwrap();
        assert_eq!(
            label(&payments.nodes, &payments.file.path, lambda),
            "anonymous function"
        );
        assert_eq!(
            qualified_name(&payments.nodes, &payments.file.path, lambda),
            "retry.anonymous function"
        );
    }

    #[test]
    fn starts_are_found_by_key() {
        let graph = sample::shop().build();
        let app = start(&graph.apps, "entrance:shop").unwrap();
        assert_eq!(app.name(), "Shop");
        assert_eq!(app.on(), Some("python -m shop.service"));
        assert_eq!(app.targets().len(), 1);
        let entrance = start(&graph.apps, "entrance:shop/charge-a-card").unwrap();
        assert_eq!(entrance.name(), "Charge a card");
        assert_eq!(entrance.app().key.as_str(), "entrance:shop");
        assert_eq!(entrance.targets().len(), 2);
        assert!(start(&graph.apps, "entrance:shop/absent").is_none());
        assert_eq!(starts(&graph.apps).count(), 1 + graph.apps[0].entrances.len());
    }
}
