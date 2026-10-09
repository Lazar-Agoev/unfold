//! Shared selection through the alien mapping: preorder IDs, parents, roles (across wrappers and
//! decorators), names, slots, keys, lines, and skipped syntax.

use unfold_core::vocab::{BlockKind as K, Role, SlotName};

use crate::alien::Project;
use crate::compile;

const SOURCE: &str = "(fn work (param a) (param b)
  (if (call (ref a))
    (do (stmt (call (ref b))))
    (ret (call (ref work)))))
(class Base :base=Root (deco (call (ref work)))
  (method ping (type (call (ref hidden))))
  (ctor))
(const handler (wrap (lambda)))
(fn (param x))
";

#[test]
fn blocks_roles_names_and_keys() {
    let compiled = compile(SOURCE);
    let file = compiled.file("a.alien");
    let summary: Vec<_> = file
        .blocks
        .iter()
        .map(|block| {
            (
                block.kind,
                block.parent,
                block.role,
                block.name.as_deref(),
                &block.key.as_str()[8..],
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (K::File, None, None, None, ""),
            (
                K::Function,
                Some(0),
                Some(Role::Body),
                Some("work"),
                "function:work"
            ),
            (K::If, Some(1), Some(Role::Body), None, "function:work/if[0]"),
            (
                K::Call,
                Some(2),
                Some(Role::Condition),
                None,
                "function:work/if[0]/call[0]"
            ),
            // The brace-only `do` is not a block; the path keeps its first slot, `then`.
            (
                K::ExpressionStatement,
                Some(2),
                Some(Role::Then),
                None,
                "function:work/if[0]/expression_statement[0]"
            ),
            (
                K::Call,
                Some(4),
                Some(Role::Expression),
                None,
                "function:work/if[0]/expression_statement[0]/call[0]"
            ),
            (
                K::Return,
                Some(2),
                Some(Role::Else),
                None,
                "function:work/if[0]/return[0]"
            ),
            (
                K::Call,
                Some(6),
                Some(Role::Expression),
                None,
                "function:work/if[0]/return[0]/call[0]"
            ),
            (K::Class, Some(0), Some(Role::Body), Some("Base"), "class:Base"),
            // A path through a decorator gets `decorator`.
            (
                K::Call,
                Some(8),
                Some(Role::Decorator),
                None,
                "class:Base/call[0]"
            ),
            // The `type` subtree is skipped, calls and all.
            (
                K::Method,
                Some(8),
                Some(Role::Body),
                Some("ping"),
                "class:Base/method:ping"
            ),
            (
                K::Constructor,
                Some(8),
                Some(Role::Body),
                Some("constructor"),
                "class:Base/constructor:constructor"
            ),
            (
                K::Variable,
                Some(0),
                Some(Role::Body),
                Some("handler"),
                "variable:handler"
            ),
            // An anonymous function takes the name of the variable it initializes, through wrappers.
            (
                K::Function,
                Some(12),
                Some(Role::Initializer),
                Some("handler"),
                "variable:handler/function:handler"
            ),
            (K::Function, Some(0), Some(Role::Body), None, "function[1]"),
        ]
    );
    assert!(
        file.blocks
            .iter()
            .enumerate()
            .all(|(index, block)| block.local_id as usize == index)
    );
    // Lines come from the shared line index; the root spans the whole file.
    assert_eq!(
        (file.blocks[0].range.start, file.blocks[0].range.end),
        (0, SOURCE.len() as u32)
    );
    assert_eq!((file.blocks[1].lines.start, file.blocks[1].lines.end), (1, 4));
    assert_eq!((file.blocks[8].lines.start, file.blocks[8].lines.end), (5, 7));
}

#[test]
fn slots_are_sorted_and_indexed() {
    let compiled = compile(SOURCE);
    let file = compiled.file("a.alien");
    let of = |local: u32| -> Vec<(SlotName, Option<u32>, &str)> {
        file.slots
            .iter()
            .filter(|slot| slot.block == local)
            .map(|slot| (slot.slot, slot.index, &SOURCE[slot.range.as_usize()]))
            .collect()
    };
    assert_eq!(
        of(1),
        [
            (SlotName::Name, None, "work"),
            (SlotName::Parameter, Some(0), "(param a)"),
            (SlotName::Parameter, Some(1), "(param b)"),
            (
                SlotName::Body,
                None,
                "(if (call (ref a))\n    (do (stmt (call (ref b))))\n    (ret (call (ref work))))"
            ),
        ]
    );
    assert_eq!(
        of(2),
        [
            (SlotName::Condition, None, "(call (ref a))"),
            (SlotName::Then, None, "(do (stmt (call (ref b))))"),
            (SlotName::Else, None, "(ret (call (ref work)))"),
        ]
    );
    assert_eq!(of(3), [(SlotName::Callee, None, "(ref a)")]);
    assert_eq!(
        of(8),
        [
            (SlotName::Name, None, "Base"),
            (SlotName::Heritage, None, ":base=Root")
        ]
    );
    assert_eq!(
        of(12),
        [
            (SlotName::Name, None, "handler"),
            (SlotName::Initializer, None, "(wrap (lambda))")
        ]
    );
    // Slots are ordered by block, then start.
    assert!(
        file.slots
            .windows(2)
            .all(|pair| (pair[0].block, pair[0].range.start) <= (pair[1].block, pair[1].range.start))
    );
}

#[test]
fn duplicate_names_and_ordinals_follow_core_keys() {
    let compiled = compile("(fn f) (fn f) (fn) (if (ref a) (ref b)) (if (ref a) (ref b))");
    let keys: Vec<&str> = compiled
        .file("a.alien")
        .blocks
        .iter()
        .map(|block| &block.key.as_str()[8..])
        .collect();
    assert_eq!(
        keys,
        ["", "function:f", "function:f[1]", "function[2]", "if[0]", "if[1]"]
    );
}

#[test]
fn deeply_nested_files_fail_cleanly() {
    let depth = 2000;
    let source = format!("{}{}", "(if (ref a) ".repeat(depth), ")".repeat(depth));
    let compiled = Project::new()
        .file("deep.alien", &source)
        .file("a.alien", "(fn ok)")
        .compile()
        .valid();
    let deep = compiled.file("deep.alien");
    assert_eq!(deep.file.state, unfold_core::vocab::FileState::Failed);
    assert!(
        deep.file
            .failure
            .as_deref()
            .is_some_and(|failure| failure.contains("nest"))
    );
    assert_eq!(compiled.file("a.alien").blocks.len(), 2);
}

#[test]
fn highlighting_is_well_formed() {
    use unfold_core::highlight::spans_are_well_formed;
    use unfold_engine::Language;
    let alien = crate::alien::Alien;
    let spans = alien.highlighter().highlight(SOURCE);
    assert!(!spans.is_empty());
    assert!(spans_are_well_formed(SOURCE, &spans));
    assert!(alien.highlighter().highlight("(unclosed").is_empty());
}
