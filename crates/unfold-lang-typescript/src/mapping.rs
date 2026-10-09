//! The TypeScript graph mapping over the normalized syntax: which nodes select blocks, their
//! roles, names, and slots. The shared selector in `unfold_engine::select` applies it.
//!
//! | Syntax | Block |
//! | --- | --- |
//! | Function declaration or expression, arrow function | `function`, only with a body |
//! | Class declaration or expression | `class`, unless ambient |
//! | Method, constructor, getter, setter (class or object literal) | the member kind, only with a body |
//! | Variable declaration (including loop heads and catch bindings) | `variable`, unless ambient |
//! | Binding element naming one identifier in a variable's pattern | `variable` |
//! | Call, tagged template | `call` |
//! | `new` | `new` |
//! | Runtime import, `export … from`, `import x = require(…)`, dynamic `import()` | `import` |
//! | Control statements | `if`, `for`, `for_in`, `for_of`, `while`, `do_while`, `switch`, `case`, `default`, `try`, `catch`, `return`, `throw`, `break`, `continue`, `expression_statement` |
//!
//! Type syntax is skipped with its whole subtree; every other unselected node is traversed.

use unfold_core::text::ByteRange;
use unfold_core::vocab::{BlockKind, Role, SlotName};
use unfold_engine::select::{MappedSlot, Mapping, Saved, Tree};

use crate::names;
use crate::syntax::{Cat, Flags, Label, Syntax};

/// The mapping over one file's normalized syntax.
pub(crate) struct TypeScriptMapping<'t, 'a> {
    pub(crate) syntax: &'t Syntax<'a>,
}

impl TypeScriptMapping<'_, '_> {
    fn selected_kind(&self, node: u32) -> Option<BlockKind> {
        let found = self.syntax.node(node);
        let body = found.flags.has(Flags::HAS_BODY);
        let ambient = found.flags.has(Flags::AMBIENT);
        let kind = match found.cat {
            Cat::SourceFile => BlockKind::File,
            Cat::FunctionDeclaration | Cat::FunctionExpression | Cat::ArrowFunction if body => {
                BlockKind::Function
            }
            Cat::ClassDeclaration | Cat::ClassExpression if !ambient => BlockKind::Class,
            Cat::Method if body => BlockKind::Method,
            Cat::Constructor if body => BlockKind::Constructor,
            Cat::Getter if body => BlockKind::Getter,
            Cat::Setter if body => BlockKind::Setter,
            Cat::VariableDeclaration if !ambient => BlockKind::Variable,
            Cat::BindingElement
                if !ambient
                    && found.flags.has(Flags::VARIABLE_BINDING)
                    && self
                        .syntax
                        .child(node, Label::Name)
                        .is_some_and(|name| self.syntax.cat(name) == Cat::Identifier) =>
            {
                BlockKind::Variable
            }
            Cat::Call | Cat::TaggedTemplate => BlockKind::Call,
            Cat::New => BlockKind::New,
            cat if (cat.is_module_declaration() || cat == Cat::ImportCall)
                && found.flags.has(Flags::RUNTIME) =>
            {
                BlockKind::Import
            }
            Cat::If => BlockKind::If,
            Cat::For => BlockKind::For,
            Cat::ForIn => BlockKind::ForIn,
            Cat::ForOf => BlockKind::ForOf,
            Cat::While => BlockKind::While,
            Cat::DoWhile => BlockKind::DoWhile,
            Cat::Switch => BlockKind::Switch,
            Cat::Case => BlockKind::Case,
            Cat::Default => BlockKind::Default,
            Cat::Try => BlockKind::Try,
            Cat::Catch => BlockKind::Catch,
            Cat::Return => BlockKind::Return,
            Cat::Throw => BlockKind::Throw,
            Cat::Break => BlockKind::Break,
            Cat::Continue => BlockKind::Continue,
            Cat::ExpressionStatement => BlockKind::ExpressionStatement,
            _ => return None,
        };
        Some(kind)
    }
}

/// The role a child slot of a saved parent maps to (the proof of concept's `roleSlots`).
fn slot_role(kind: BlockKind, slot: Label) -> Option<Role> {
    use BlockKind as K;
    use Label as L;
    let role = match (kind, slot) {
        (K::File, L::Statements) => Role::Body,
        (kind, L::Body) if kind.is_function_like() => Role::Body,
        (kind, L::Parameters) if kind.is_function_like() => Role::Parameter,
        (K::If, L::Expression) => Role::Condition,
        (K::If, L::ThenStatement) => Role::Then,
        (K::If, L::ElseStatement) => Role::Else,
        (K::For | K::ForIn | K::ForOf, L::Initializer) => Role::Initializer,
        (K::For, L::Condition) => Role::Condition,
        (K::For, L::Incrementor) => Role::Update,
        (K::For | K::ForIn | K::ForOf | K::While | K::DoWhile, L::Statement) => Role::Body,
        (K::ForIn | K::ForOf, L::Expression) => Role::Iterable,
        (K::While | K::DoWhile, L::Expression) => Role::Condition,
        (K::Try, L::TryBlock) => Role::Body,
        (K::Try, L::CatchClause) => Role::Catch,
        (K::Try, L::FinallyBlock) => Role::Finally,
        (K::Return | K::Throw | K::ExpressionStatement | K::Switch, L::Expression) => Role::Expression,
        (K::Variable, L::Initializer) => Role::Initializer,
        (K::Call | K::New, L::Expression) => Role::Callee,
        (K::Call | K::New, L::Arguments) => Role::Argument,
        (K::Switch, L::CaseBlock) => Role::Body,
        (K::Case | K::Default, L::Expression) => Role::Condition,
        (K::Case | K::Default, L::Statements) => Role::Body,
        (K::Catch, L::VariableDeclaration) => Role::Binding,
        (K::Catch, L::Block) => Role::Body,
        (K::Class, L::Members) => Role::Body,
        (K::Class, L::HeritageClauses) => Role::Heritage,
        _ => return None,
    };
    Some(role)
}

/// The slot a child of a block of `kind` fills (the proof of concept's `slotFields`).
fn slot_name(kind: BlockKind, cat: Cat, label: Label) -> Option<SlotName> {
    use BlockKind as K;
    use Label as L;
    let slot = match (kind, label) {
        (kind, L::Name) if kind.is_function_like() => SlotName::Name,
        (kind, L::Parameters) if kind.is_function_like() => SlotName::Parameter,
        (kind, L::Type) if kind.is_function_like() => SlotName::ReturnType,
        (kind, L::Body) if kind.is_function_like() => SlotName::Body,
        (K::Class | K::Variable, L::Name) => SlotName::Name,
        (K::Class, L::HeritageClauses) => SlotName::Heritage,
        (K::Variable | K::For | K::ForIn | K::ForOf, L::Initializer) => SlotName::Initializer,
        (K::Call | K::New, L::Expression) | (K::Call, L::Tag) => SlotName::Callee,
        // A tagged template has no argument slots.
        (K::Call, L::Arguments) if cat != Cat::TaggedTemplate => SlotName::Argument,
        (K::New, L::Arguments) => SlotName::Argument,
        (K::If | K::While | K::DoWhile | K::Case, L::Expression) | (K::For, L::Condition) => {
            SlotName::Condition
        }
        (K::If, L::ThenStatement) => SlotName::Then,
        (K::If, L::ElseStatement) => SlotName::Else,
        (K::For, L::Incrementor) => SlotName::Update,
        (K::For | K::ForIn | K::ForOf | K::While | K::DoWhile, L::Statement) => SlotName::Body,
        (K::ForIn | K::ForOf, L::Expression) => SlotName::Iterable,
        (K::Switch | K::Return | K::Throw | K::ExpressionStatement, L::Expression) => SlotName::Expression,
        (K::Switch, L::CaseBlock) => SlotName::Body,
        (K::Try, L::TryBlock) => SlotName::Body,
        (K::Try, L::CatchClause) => SlotName::Catch,
        (K::Try, L::FinallyBlock) => SlotName::Finally,
        (K::Catch, L::VariableDeclaration) => SlotName::Binding,
        (K::Catch, L::Block) => SlotName::Body,
        _ => return None,
    };
    Some(slot)
}

impl Mapping for TypeScriptMapping<'_, '_> {
    type Node = u32;
    type Slot = Label;

    fn range(&self, node: u32) -> ByteRange {
        self.syntax.range(node)
    }

    fn skip(&self, node: u32) -> bool {
        self.syntax.cat(node) == Cat::Type
    }

    fn decorator(&self, node: u32) -> bool {
        self.syntax.cat(node) == Cat::Decorator
    }

    fn kind(&self, node: u32) -> Option<BlockKind> {
        self.selected_kind(node)
    }

    fn name(&self, node: u32, kind: BlockKind) -> Option<Box<str>> {
        names::block_name(self.syntax, node, kind).map(Box::from)
    }

    fn role(&self, parent: Saved<u32>, first_slot: Option<Label>, through_decorator: bool) -> Option<Role> {
        if through_decorator {
            return Some(Role::Decorator);
        }
        let slot = first_slot?;
        if slot == Label::Name {
            let name = self
                .syntax
                .child(parent.node, Label::Name)
                .map(|name| self.syntax.cat(name));
            if parent.kind == BlockKind::Variable && name == Some(Cat::BindingPattern) {
                return Some(Role::Binding);
            }
            if (parent.kind == BlockKind::Class || parent.kind.is_function_like())
                && name == Some(Cat::ComputedPropertyName)
            {
                return Some(Role::Name);
            }
        }
        slot_role(parent.kind, slot)
    }

    fn slots(&self, node: u32, kind: BlockKind, out: &mut Vec<MappedSlot>) {
        let syntax = self.syntax;
        if kind == BlockKind::Import {
            let specifier = match syntax.cat(node) {
                Cat::ImportCall => syntax.child(node, Label::Arguments),
                _ => syntax.child(node, Label::ModuleSpecifier),
            };
            if let Some(specifier) = specifier {
                out.push(MappedSlot {
                    slot: SlotName::Specifier,
                    index: None,
                    range: syntax.range(specifier),
                });
            }
            return;
        }
        let cat = syntax.cat(node);
        let mut parameters = 0;
        let mut arguments = 0;
        let mut heritage = false;
        let mut statements: Option<ByteRange> = None;
        for child in syntax.children(node) {
            let found = syntax.node(child);
            if matches!(kind, BlockKind::Case | BlockKind::Default) && found.label == Label::Statements {
                // A clause's body runs from its first statement to its last.
                let span = statements.get_or_insert(found.range);
                span.end = found.range.end;
                continue;
            }
            let Some(slot) = slot_name(kind, cat, found.label) else {
                continue;
            };
            let index = match slot {
                SlotName::Parameter => {
                    parameters += 1;
                    Some(parameters - 1)
                }
                SlotName::Argument => {
                    arguments += 1;
                    Some(arguments - 1)
                }
                // Only the first non-type (`extends`) clause is a heritage slot.
                SlotName::Heritage if heritage || found.cat == Cat::Type => continue,
                SlotName::Heritage => {
                    heritage = true;
                    None
                }
                _ => None,
            };
            out.push(MappedSlot {
                slot,
                index,
                range: found.range,
            });
        }
        if let Some(range) = statements {
            out.push(MappedSlot {
                slot: SlotName::Body,
                index: None,
                range,
            });
        }
    }
}

impl Tree for TypeScriptMapping<'_, '_> {
    fn children(&self, node: u32, out: &mut Vec<(Label, u32)>) {
        out.extend(
            self.syntax
                .children(node)
                .map(|child| (self.syntax.node(child).label, child)),
        );
    }
}
