//! The value table and occurrence facts: one [`CallFact`] per call, tagged template, and
//! construction block, one [`ImportFact`] per import block, and a [`ValueFact`] for every callee,
//! argument, and the receivers, callees, and tags they depend on.
//!
//! | Unwrapped expression | Shape | Label |
//! | --- | --- | --- |
//! | Identifier | `Name` | its text |
//! | Property access (`a.b`, `a?.b`, `a.#b`) | `Member` | `receiver.name` |
//! | Element access with a literal key | `Member` | `receiver["key"]` as written |
//! | Other element access | `Other` | `receiver[…]` |
//! | Call (or dynamic import) used as a value | `CallResult` | `callee()` |
//! | Function or arrow expression | `InlineFunction` | `(…)` |
//! | Class expression | `InlineClass` | `(…)` |
//! | `new`, tagged template | `Other` | the target's or tag's label |
//! | `this`, `super` | `Other` | the keyword |
//! | Anything else, including spreads | `Other` | `(…)` |
//!
//! Parentheses, `!`, `as`, `satisfies`, and `<T>` assertions are transparent. Values are created
//! dependencies first, so a receiver always precedes the access that uses it.

use rustc_hash::FxHashMap;
use unfold_core::vocab::{EdgeKind, Expansion};
use unfold_engine::facts::{ArgumentFact, CallContext, CallFact, ImportFact, ValueFact, ValueShape};

use crate::names;
use crate::payload::{ImportKind, ValueSyntax};
use crate::syntax::{Cat, Label, NONE, Syntax};

/// Everything recorded about one file's occurrences.
#[derive(Debug, Default)]
pub(crate) struct Occurrences {
    pub(crate) values: Vec<ValueFact>,
    pub(crate) syntax: Vec<ValueSyntax>,
    pub(crate) calls: Vec<CallFact>,
    pub(crate) imports: Vec<ImportFact>,
    pub(crate) import_kinds: Vec<ImportKind>,
    /// The syntax node each value was recorded for (before transparent wrappers are removed).
    pub(crate) nodes: Vec<u32>,
}

/// Records the occurrences of every selected call, construction, and import. `block_of` maps a
/// node to its block's local ID, or [`NONE`]. More values can be added to the returned table
/// ([`Table::value`]) before it is finished.
pub(crate) fn occurrences<'t, 'a>(
    syntax: &'t Syntax<'a>,
    source: &'t str,
    block_of: &'t [u32],
) -> Table<'t, 'a> {
    let mut table = Table {
        syntax,
        source,
        block_of,
        memo: FxHashMap::default(),
        out: Occurrences::default(),
    };
    for node in 0..syntax.len() as u32 {
        let block = block_of[node as usize];
        if block == NONE {
            continue;
        }
        match syntax.cat(node) {
            Cat::Call | Cat::TaggedTemplate | Cat::New => table.call(node, block),
            Cat::ImportDeclaration | Cat::Reexport | Cat::ImportEquals | Cat::ImportCall => {
                table.import(node, block);
            }
            _ => {}
        }
    }
    table
}

/// The value table being recorded.
pub(crate) struct Table<'t, 'a> {
    syntax: &'t Syntax<'a>,
    source: &'t str,
    block_of: &'t [u32],
    memo: FxHashMap<u32, u32>,
    out: Occurrences,
}

impl Table<'_, '_> {
    fn call(&mut self, node: u32, block: u32) {
        let syntax = self.syntax;
        let cat = syntax.cat(node);
        let callee_label = if cat == Cat::TaggedTemplate {
            Label::Tag
        } else {
            Label::Expression
        };
        let callee_node = syntax.child(node, callee_label);
        let callee = callee_node.map(|callee| self.value(callee));
        let mut arguments = Vec::new();
        if cat != Cat::TaggedTemplate {
            let written: Vec<u32> = syntax
                .children(node)
                .filter(|&child| syntax.node(child).label == Label::Arguments)
                .collect();
            for (position, argument) in written.into_iter().enumerate() {
                // A spread can never supply a named reference.
                let spread = syntax.cat(argument) == Cat::Spread;
                arguments.push(ArgumentFact {
                    expand: spread.then_some(Expansion::Spread),
                    ..ArgumentFact::positional(self.value(argument), position as u32, !spread)
                });
            }
        }
        // `super(…)` keeps its call block and `calls` edge but selects a construction target.
        let construct = cat == Cat::New
            || (cat == Cat::Call && callee_node.is_some_and(|callee| syntax.cat(callee) == Cat::Super));
        self.out.calls.push(CallFact {
            block,
            callee,
            edge: if cat == Cat::New {
                EdgeKind::Constructs
            } else {
                EdgeKind::Calls
            },
            context: if construct {
                CallContext::Construct
            } else {
                CallContext::Call
            },
            arguments,
        });
    }

    fn import(&mut self, node: u32, block: u32) {
        let syntax = self.syntax;
        let (specifier, kind) = match syntax.cat(node) {
            // Only a direct string or plain template literal names the module; a parenthesized
            // literal is a non-literal dependency.
            Cat::ImportCall => (
                syntax.child(node, Label::Arguments).filter(|&argument| {
                    matches!(
                        syntax.cat(argument),
                        Cat::StringLiteral | Cat::NoSubstitutionTemplate
                    )
                }),
                ImportKind::Dynamic,
            ),
            Cat::Reexport => (syntax.child(node, Label::ModuleSpecifier), ImportKind::Reexport),
            Cat::ImportEquals => (
                syntax.child(node, Label::ModuleSpecifier),
                ImportKind::ImportEquals,
            ),
            _ => (syntax.child(node, Label::ModuleSpecifier), ImportKind::Import),
        };
        let specifier = specifier.map(|literal| Box::from(syntax.node(literal).text));
        self.out.import_kinds.push(kind);
        self.out.imports.push(ImportFact { block, specifier });
    }

    /// The syntax node of each value recorded so far.
    pub(crate) fn nodes(&self) -> &[u32] {
        &self.out.nodes
    }

    /// The recorded occurrences and values.
    pub(crate) fn finish(self) -> Occurrences {
        self.out
    }

    /// The value index of `node`, creating it and everything it depends on.
    pub(crate) fn value(&mut self, node: u32) -> u32 {
        // Walk the dependency chain (receiver, callee, or tag) iteratively, then create values
        // innermost first, so long access chains never recurse.
        let mut chain = Vec::new();
        let mut current = Some(node);
        while let Some(link) = current {
            if self.memo.contains_key(&link) {
                break;
            }
            chain.push(link);
            current = self.dependency(link);
        }
        let mut dependency = current.and_then(|link| self.memo.get(&link).copied());
        for &link in chain.iter().rev() {
            let index = self.create(link, dependency);
            self.memo.insert(link, index);
            dependency = Some(index);
        }
        dependency.unwrap_or_default()
    }

    /// The expression a value's label and facts are built from.
    fn dependency(&self, node: u32) -> Option<u32> {
        let syntax = self.syntax;
        let unwrapped = syntax.unwrap(node);
        match syntax.cat(unwrapped) {
            Cat::PropertyAccess | Cat::ElementAccess | Cat::Call | Cat::New => {
                syntax.child(unwrapped, Label::Expression)
            }
            Cat::TaggedTemplate => syntax.child(unwrapped, Label::Tag),
            _ => None,
        }
    }

    fn create(&mut self, node: u32, dependency: Option<u32>) -> u32 {
        let syntax = self.syntax;
        let unwrapped = syntax.unwrap(node);
        let found = syntax.node(unwrapped);
        let label = {
            let inner = dependency.map(|index| &*self.out.values[index as usize].label);
            names::label_from(syntax, self.source, node, inner)
        };
        let block = self.block_of[unwrapped as usize];
        let (shape, info) = match found.cat {
            Cat::Identifier => (ValueShape::Name, ValueSyntax::Name { range: found.range }),
            Cat::PropertyAccess => (ValueShape::Member, ValueSyntax::Member),
            Cat::ElementAccess => {
                let key = syntax
                    .child(unwrapped, Label::ArgumentExpression)
                    .map(|key| syntax.unwrap(key))
                    .filter(|&key| syntax.cat(key).is_name_literal());
                match key {
                    Some(key) => (
                        ValueShape::Member,
                        ValueSyntax::LiteralKey {
                            key: syntax.node(key).text.into(),
                        },
                    ),
                    None => (ValueShape::Other, ValueSyntax::Computed),
                }
            }
            Cat::Call | Cat::ImportCall => (
                ValueShape::CallResult,
                ValueSyntax::CallResult { callee: dependency },
            ),
            Cat::New => (
                ValueShape::Other,
                ValueSyntax::Construction { callee: dependency },
            ),
            Cat::TaggedTemplate => (ValueShape::Other, ValueSyntax::TaggedTemplate { tag: dependency }),
            Cat::This => (ValueShape::Other, ValueSyntax::This { range: found.range }),
            Cat::Super => (ValueShape::Other, ValueSyntax::Super { range: found.range }),
            Cat::FunctionExpression | Cat::ArrowFunction if block != NONE => {
                (ValueShape::InlineFunction, ValueSyntax::Inline)
            }
            Cat::ClassExpression if block != NONE => (ValueShape::InlineClass, ValueSyntax::Inline),
            _ => (ValueShape::Other, ValueSyntax::Other),
        };
        let mut fact = ValueFact::new(shape, &label);
        if matches!(found.cat, Cat::PropertyAccess | Cat::ElementAccess) {
            fact.receiver = dependency;
        }
        if found.cat == Cat::PropertyAccess {
            fact.member = syntax
                .child(unwrapped, Label::Name)
                .and_then(|name| names::declared_name(syntax, name))
                .map(Box::from);
        }
        if info == ValueSyntax::Inline {
            fact.block = Some(block);
        }
        let index = self.out.values.len() as u32;
        self.out.values.push(fact);
        self.out.syntax.push(info);
        self.out.nodes.push(node);
        index
    }
}
