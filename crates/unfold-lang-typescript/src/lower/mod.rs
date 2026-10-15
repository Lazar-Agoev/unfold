//! Lowering: oxc's parse tree straight to the file's nodes (docs/languages/typescript.md#lowering),
//! with one rule per syntax form, keyed with
//! [`KeyBuilder`] in preorder. The same walk records each call's facts and the value table, each
//! written module dependency, each function's signature and parameters, the members of types
//! and classes, and the node each function, class, method, and type declaration became, which
//! binding reads ([`Lowered::definitions`]).
//!
//! | Written | Node |
//! | --- | --- |
//! | the file | `module`; a leading file JSDoc is its `doc` |
//! | function declarations; methods, constructors, accessors (class or object literal) | `function` (`function`, `method`, `constructor`, `getter`, `setter`) |
//! | arrow functions, function expressions | `function` (`lambda`): `callback[n]` in a call's arguments; named by `const x = …`, a class property, a property of an object literal that isn't an argument (after the names of what names the object), or `export default`; `lambda[n]` anywhere else |
//! | `class`; `interface`, `type`, `enum` | `class`; `type` (`interface`, `type_alias`, `enum`); a class expression nothing names is an inline definition, `callback[n]` or `lambda[n]` with the form `class` |
//! | decorators | calls in the enclosing body, role `decorator`, `for` the definition, with no `stmt`: the definition's statement runs from its first decorator |
//! | a class's `extends` call | a call in the enclosing body, role `base`, `for` the class |
//! | parameter defaults | calls at the start of the function's body, role `default` |
//! | `if` / `else if` / `else` | `decision` (`if`): `then`, `elif[n]`, `else`; test calls have role `test` |
//! | `c ? a : b` with a call | `decision` (`ternary`): `then` (test `c`), `else` |
//! | `switch` | `decision` (`switch`): `case[n]`, `default`; a case without a final `break`, `return`, `throw`, or `continue` before another case `falls_through` |
//! | `for`, `for … in`, `for … of`, `while`, `do … while` | `loop` (`for`, `while`, `do`) with `body`; header calls in the loop |
//! | `try` / `catch` / `finally` | `try` with `body`, `catch`, `finally` |
//! | `using`, `await using` | `with` (`using`): its header the declaration, its `body` the rest of the block |
//! | `return`, `throw`, `break`, `continue`; `yield` as a statement or an assignment's whole value | `exit`; `break` and `continue` target their loop (a `break` may target its `switch`), labels resolved; a `break` that ends a case is no node |
//! | calls, tagged templates, `new`, `import()` | `call` (`new` for `new`); `import()` with an `imports` link |
//! | `&&`, `\|\|`, `??`, `?.` | their calls are `conditional` |
//! | declarations without a call, expression statements, imports, JSX, type syntax | code: their calls are nodes, imports are links, types are typed records |
//!
//! What a front end can't place is marked `truncated` on the nearest node above it: nesting past
//! [`MAX_DEPTH`] (the code below is left out, its range recorded as a cut that the census
//! skips), and a `break` out of a labeled plain block. A `break` that leaves a `switch` before
//! its case ends is an exit whose target is the `switch`.

mod define;
mod expressions;
mod forms;
mod statements;

pub(crate) use define::decorator_is_call;
pub(crate) use forms::{CalleeStep, callee_step};

use oxc_allocator::Address;
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AssignmentOperator, AwaitExpression, BreakStatement,
    CallExpression, Class, Comment, ComputedMemberExpression, ConditionalExpression, ContinueStatement,
    Decorator, DoWhileStatement, ExportAllDeclaration, ExportDeclaration, ExportDefaultDeclaration,
    ExportFromDeclaration, ExportNamedDeclaration, Expression, ForInStatement, ForOfStatement, ForStatement,
    FormalParameter, FormalParameterRest, Function, IfStatement, ImportDeclaration, ImportExpression,
    LabeledStatement, LogicalExpression, NewExpression, ObjectExpression, Program, ReturnStatement,
    Statement, SwitchStatement, TSClassImplements, TSEnumDeclaration, TSExportAssignment,
    TSExternalModuleDeclaration, TSGlobalDeclaration, TSImportEqualsDeclaration, TSIndexSignature,
    TSInterfaceDeclaration, TSNamespaceDeclaration, TSType, TSTypeAliasDeclaration, TSTypeAnnotation,
    TSTypeParameterDeclaration, TSTypeParameterInstantiation, TaggedTemplateExpression, ThrowStatement,
    TryStatement, VariableDeclaration, WhileStatement, YieldExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Token;
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeFlags;
use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::key::{Child, KeyBuilder, KeyError};
use unfold_core::text::{ByteRange, LineBreaks, LineIndex};
use unfold_core::vocab::{CallRole, Flag, Flags, Form, NodeKind};
use unfold_core::{Node, Snapshot};
use unfold_engine::facts::{CallFact, ImportFact, TypeFacts};

use crate::payload::ImportKind;
use crate::spans::{Tokens, range};
use crate::typed::{Owners, Typed};
use crate::values::Values;

/// The deepest a node may nest under the module. A node that would nest deeper is left out,
/// with everything inside it, and the node it would sit in is marked `truncated`.
pub(crate) const MAX_DEPTH: u32 = 256;

/// One file, lowered.
pub(crate) struct Lowered {
    /// The nodes in preorder; the module is 0.
    pub(crate) nodes: Vec<Node>,
    /// The node each function, class, method, object-literal method, and type declaration
    /// became, by its oxc arena address.
    pub(crate) definitions: FxHashMap<Address, u32>,
    /// The value table.
    pub(crate) values: Values,
    /// One per call node, in node order.
    pub(crate) calls: Vec<CallFact>,
    /// The written module dependencies, in written order.
    pub(crate) imports: Vec<ImportFact>,
    /// How each module dependency is written, aligned with `imports`.
    pub(crate) import_kinds: Vec<ImportKind>,
    /// Signatures, parameters, and members.
    pub(crate) types: TypeFacts,
    /// The written types whose names get `types` and `extends` links.
    pub(crate) linked: FxHashSet<ByteRange>,
    /// The node each of their type names, and each heritage clause's, links from.
    pub(crate) owners: Owners,
    /// The ranges of the syntax left out past the depth limit, sorted and disjoint.
    pub(crate) cuts: Vec<ByteRange>,
}

/// Lowers one parsed file. `verbatim` is TypeScript's `verbatimModuleSyntax`, which decides
/// which imports are runtime dependencies.
pub(crate) fn lower(
    program: &Program<'_>,
    snapshot: &Snapshot,
    tokens: &[Token],
    verbatim: bool,
) -> Result<Lowered, KeyError> {
    let source = snapshot.text();
    let lines = snapshot.line_index(LineBreaks::EcmaScript);
    let keys = KeyBuilder::new(snapshot.path());
    let module_range = ByteRange {
        start: 0,
        end: snapshot.len(),
    };
    let module = Node {
        key: keys.keys()[0].clone(),
        parent: None,
        form: Form::Module,
        name: None,
        range: module_range,
        lines: lines.span(module_range),
        head: None,
        body: None,
        doc: None,
        stmt: None,
        value: None,
        binds: None,
        role: None,
        r#for: None,
        flags: Flags::NONE,
        target: None,
    };
    let tokens = Tokens(tokens);
    let mut lowering = Lowering {
        source,
        tokens,
        comments: &program.comments,
        lines,
        verbatim,
        keys,
        nodes: vec![module],
        depths: vec![0],
        definitions: FxHashMap::default(),
        values: Values::default(),
        calls: Vec::new(),
        imports: Vec::new(),
        import_kinds: Vec::new(),
        typed: Typed::new(source, tokens),
        cuts: Vec::new(),
        claimed: FxHashSet::default(),
        error: None,
        at: At {
            parent: 0,
            within: Within::Body,
        },
        definition: 0,
        conditional: false,
        statement: None,
        enclosing: None,
        prefix: Vec::new(),
        export: None,
        breakables: Vec::new(),
        labels: Vec::new(),
        final_breaks: FxHashSet::default(),
        binds: FxHashMap::default(),
        exit_values: FxHashSet::default(),
        awaited: FxHashSet::default(),
        yields: FxHashSet::default(),
    };
    lowering.statements(&program.body);
    lowering.nodes[0].doc = lowering.file_doc(program);
    if let Some(error) = lowering.error {
        return Err(error);
    }
    let (types, linked, owners) = lowering.typed.finish();
    let mut cuts = lowering.cuts;
    cuts.sort_unstable_by_key(|cut| (cut.start, cut.end));
    Ok(Lowered {
        nodes: lowering.nodes,
        definitions: lowering.definitions,
        values: lowering.values,
        calls: lowering.calls,
        imports: lowering.imports,
        import_kinds: lowering.import_kinds,
        types,
        linked,
        owners,
        cuts,
    })
}

/// What the code being lowered is to the node it sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Within {
    /// The statements of a definition or a part, or class-level code.
    Body,
    /// A part's test: its calls have the role `test`.
    Test,
    /// A control block's header.
    Header,
    /// A call's arguments: an inline definition reached here is a callback.
    Argument,
    /// Any other expression a node holds: a call's receiver, an exit's value.
    Value,
    /// A function's parameter defaults: its calls have the role `default`.
    Default,
    /// A decorator: its call has the role `decorator`.
    Decorator,
    /// A class's `extends` expression: its call has the role `base`.
    Base,
}

impl Within {
    /// The role of a call written directly here.
    const fn role(self) -> Option<CallRole> {
        match self {
            Self::Test => Some(CallRole::Test),
            Self::Default => Some(CallRole::Default),
            Self::Decorator => Some(CallRole::Decorator),
            Self::Base => Some(CallRole::Base),
            Self::Body | Self::Header | Self::Argument | Self::Value => None,
        }
    }
}

/// Where the code being lowered goes.
#[derive(Debug, Clone, Copy)]
struct At {
    parent: u32,
    within: Within,
}

/// What a `break` can leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Breaks {
    /// A loop node.
    Loop(u32),
    /// A `switch` decision node.
    Switch(u32),
    /// Any other labeled statement.
    Plain,
}

/// A statement a `break` or `continue` can name, with its labels.
#[derive(Debug, Clone)]
struct Breakable {
    breaks: Breaks,
    labels: Vec<Box<str>>,
}

/// The context a function or class body starts afresh, saved while it is lowered.
struct Saved {
    at: At,
    definition: u32,
    conditional: bool,
    statement: Option<ByteRange>,
    enclosing: Option<ByteRange>,
    export: Option<u32>,
    breakables: Vec<Breakable>,
    labels: Vec<Box<str>>,
}

/// The lowering of one file.
struct Lowering<'s> {
    source: &'s str,
    tokens: Tokens<'s>,
    comments: &'s [Comment],
    lines: LineIndex,
    verbatim: bool,
    keys: KeyBuilder,
    nodes: Vec<Node>,
    /// Each node's depth under the module.
    depths: Vec<u32>,
    definitions: FxHashMap<Address, u32>,
    values: Values,
    calls: Vec<CallFact>,
    imports: Vec<ImportFact>,
    import_kinds: Vec<ImportKind>,
    typed: Typed<'s>,
    cuts: Vec<ByteRange>,
    /// The starts of the JSDoc comments definitions took as their docs.
    claimed: FxHashSet<u32>,
    /// A key that couldn't be derived, which stops the lowering.
    error: Option<KeyError>,
    at: At,
    /// The definition whose code is being lowered.
    definition: u32,
    /// Whether the code being lowered runs only sometimes inside its expression.
    conditional: bool,
    /// The statement whose first node is still to come.
    statement: Option<ByteRange>,
    /// The statement being lowered.
    enclosing: Option<ByteRange>,
    /// The names of the namespaces around the code being lowered, outermost first.
    prefix: Vec<String>,
    /// The start of the `export` keyword of the declaration being lowered.
    export: Option<u32>,
    breakables: Vec<Breakable>,
    /// The labels of the statement being lowered.
    labels: Vec<Box<str>>,
    /// The `break` statements that end a `switch` case, which are no nodes.
    final_breaks: FxHashSet<Address>,
    /// The calls, constructions, and conditional expressions whose value a statement assigns,
    /// with the written target.
    binds: FxHashMap<Address, Box<str>>,
    /// Those whose value is exactly what their exit returns, throws, or yields.
    exit_values: FxHashSet<Address>,
    /// The calls written under `await`.
    awaited: FxHashSet<Address>,
    /// The `yield` expressions that are exits ([`forms::statement_yields`]).
    yields: FxHashSet<Address>,
}

impl Lowering<'_> {
    /// Adds a node under `parent` and returns its local ID; `None` when it would nest deeper
    /// than [`MAX_DEPTH`] (then `parent` is truncated and `range` cut) or its key can't be
    /// derived. The pending statement goes to the first node it holds.
    fn push(
        &mut self,
        parent: u32,
        child: Child<'_>,
        form: Form,
        name: Option<&str>,
        range: ByteRange,
    ) -> Option<u32> {
        if self.error.is_some() {
            return None;
        }
        // A control block needs room for its parts.
        let room = u32::from(form.kind().is_control());
        let depth = self.depths[parent as usize] + 1;
        if depth + room > MAX_DEPTH {
            self.truncate(parent);
            self.cuts.push(range);
            return None;
        }
        let key = match self.keys.push(parent, child) {
            Ok(key) => key,
            Err(error) => {
                self.error = Some(error);
                return None;
            }
        };
        let stmt = match form.kind() {
            NodeKind::Part => None,
            _ => self.statement.filter(|statement| statement.contains(range)),
        };
        if stmt.is_some() {
            self.statement = None;
        }
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            key,
            parent: Some(parent),
            form,
            name: name.map(Box::from),
            range,
            lines: self.lines.span(range),
            head: None,
            body: None,
            doc: None,
            stmt,
            value: None,
            binds: None,
            role: None,
            r#for: None,
            flags: Flags::NONE,
            target: None,
        });
        self.depths.push(depth);
        Some(id)
    }

    /// A part of `form` under control block `control`.
    fn part(
        &mut self,
        control: u32,
        form: Form,
        range: ByteRange,
        head: Option<ByteRange>,
        body: Option<ByteRange>,
    ) -> Option<u32> {
        let id = self.push(control, Child::Form(form), form, None, range)?;
        let node = self.node(id);
        node.head = head;
        node.body = body;
        Some(id)
    }

    fn node(&mut self, id: u32) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    /// Marks `node` as placed only in part: something under it couldn't be lowered.
    fn truncate(&mut self, node: u32) {
        self.nodes[node as usize].flags.insert(Flag::Truncated);
    }

    /// Lowers `inside` as code `within` node `parent`, then returns to where it was.
    fn within(&mut self, parent: u32, within: Within, inside: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.at, At { parent, within });
        inside(self);
        self.at = outer;
    }

    /// Starts the body of a definition (`at.parent`): nothing of the code around it carries in.
    fn enter(&mut self, at: At) -> Saved {
        Saved {
            definition: std::mem::replace(&mut self.definition, at.parent),
            at: std::mem::replace(&mut self.at, at),
            conditional: std::mem::take(&mut self.conditional),
            statement: self.statement.take(),
            enclosing: self.enclosing.take(),
            export: self.export.take(),
            breakables: std::mem::take(&mut self.breakables),
            labels: std::mem::take(&mut self.labels),
        }
    }

    fn leave(&mut self, saved: Saved) {
        self.at = saved.at;
        self.definition = saved.definition;
        self.conditional = saved.conditional;
        self.statement = saved.statement;
        self.enclosing = saved.enclosing;
        self.export = saved.export;
        self.breakables = saved.breakables;
        self.labels = saved.labels;
    }

    /// The JSDoc comment written just before `offset`, taken as a definition's doc: the nearest
    /// one, past line comments (`// eslint-disable-next-line`), as TypeScript reads it, with
    /// only whitespace and no blank line between each comment and what follows it.
    fn doc_before(&mut self, offset: u32) -> Option<ByteRange> {
        let mut index = self
            .comments
            .partition_point(|comment| comment.span.end <= offset);
        let mut end = offset;
        loop {
            index = index.checked_sub(1)?;
            let comment = self.comments[index];
            let between = self.source.get(comment.span.end as usize..end as usize)?;
            let adjacent = between.chars().all(char::is_whitespace) && between.matches('\n').count() <= 1;
            if !adjacent {
                return None;
            }
            if comment.is_jsdoc() {
                self.claimed.insert(comment.span.start);
                return Some(range(comment.span));
            }
            if !comment.is_line() {
                return None;
            }
            end = comment.span.start;
        }
    }

    /// The file's doc: its first comment, when it is a JSDoc with only whitespace (or a
    /// shebang) before it that no definition took.
    fn file_doc(&self, program: &Program<'_>) -> Option<ByteRange> {
        let comment = self.comments.first()?;
        let start = program.hashbang.as_ref().map_or(0, |hashbang| hashbang.span.end);
        let before = self.source.get(start as usize..comment.span.start as usize)?;
        let blank = before
            .chars()
            .all(|character| character.is_whitespace() || character == '\u{feff}');
        (blank && comment.is_jsdoc() && !self.claimed.contains(&comment.span.start))
            .then(|| range(comment.span))
    }

    /// Lowers a list of statements. A `using` declaration is a `with` block whose body is the
    /// rest of the list.
    fn statements(&mut self, list: &[Statement<'_>]) {
        for (index, statement) in list.iter().enumerate() {
            if let Statement::VariableDeclaration(declaration) = statement
                && declaration.kind.is_using()
                && !declaration.declare
            {
                self.using(declaration, &list[index + 1..]);
                return;
            }
            self.visit_statement(statement);
        }
    }

    /// Links a declaration's or class field's annotation from the definition it names
    /// (`defined`) when that definition begins the statement, which then holds the annotation;
    /// else from the definition being lowered.
    fn annotate(&mut self, annotation: Option<&TSTypeAnnotation<'_>>, defined: Option<u32>) {
        let Some(annotation) = annotation else {
            return;
        };
        let written = range(annotation.span);
        let owner = defined
            .filter(|&id| {
                self.nodes[id as usize]
                    .stmt
                    .is_some_and(|stmt| stmt.contains(written))
            })
            .unwrap_or(self.definition);
        self.typed.annotation(Some(annotation), owner);
    }

    /// The names of a definition named `name` here: after the namespaces around it (`N.f`).
    fn qualified(&self, name: &str) -> Vec<String> {
        let mut names = self.prefix.clone();
        names.push(name.to_owned());
        names
    }

    /// The written text at `range`.
    fn text(&self, range: ByteRange) -> &str {
        self.source
            .get(range.start as usize..range.end as usize)
            .unwrap_or_default()
    }
}

impl<'a> Visit<'a> for Lowering<'_> {
    fn visit_statements(&mut self, it: &oxc_allocator::Vec<'a, Statement<'a>>) {
        self.statements(it);
    }

    fn visit_statement(&mut self, it: &Statement<'a>) {
        let span = range(it.span());
        let enclosing = self.enclosing.replace(span);
        self.statement = Some(span);
        self.yields.extend(forms::statement_yields(it));
        walk::walk_statement(self, it);
        self.statement = None;
        self.enclosing = enclosing;
    }

    // --- Statements -------------------------------------------------------------------------

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.if_statement(it);
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.switch_statement(it);
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.for_statement(it);
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.for_in_statement(it);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.for_of_statement(it);
    }

    fn visit_while_statement(&mut self, it: &WhileStatement<'a>) {
        self.while_statement(it);
    }

    fn visit_do_while_statement(&mut self, it: &DoWhileStatement<'a>) {
        self.do_while_statement(it);
    }

    fn visit_try_statement(&mut self, it: &TryStatement<'a>) {
        self.try_statement(it);
    }

    fn visit_labeled_statement(&mut self, it: &LabeledStatement<'a>) {
        self.labeled_statement(it);
    }

    fn visit_return_statement(&mut self, it: &ReturnStatement<'a>) {
        self.exit(Form::Return, range(it.span), it.argument.as_ref(), None);
    }

    fn visit_throw_statement(&mut self, it: &ThrowStatement<'a>) {
        self.exit(Form::Throw, range(it.span), Some(&it.argument), None);
    }

    fn visit_break_statement(&mut self, it: &BreakStatement<'a>) {
        self.break_statement(it);
    }

    fn visit_continue_statement(&mut self, it: &ContinueStatement<'a>) {
        self.continue_statement(it);
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        self.variable_declaration(it);
    }

    // --- Definitions ------------------------------------------------------------------------

    fn visit_function(&mut self, it: &Function<'a>, _flags: ScopeFlags) {
        self.function(it, None, None);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        self.arrow(it, None, None);
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        self.class(it, None, None);
    }

    fn visit_object_expression(&mut self, it: &ObjectExpression<'a>) {
        self.object(it, &[]);
    }

    fn visit_ts_interface_declaration(&mut self, it: &TSInterfaceDeclaration<'a>) {
        self.interface(it);
    }

    fn visit_ts_type_alias_declaration(&mut self, it: &TSTypeAliasDeclaration<'a>) {
        self.type_alias(it);
    }

    fn visit_ts_enum_declaration(&mut self, it: &TSEnumDeclaration<'a>) {
        self.enumeration(it);
    }

    fn visit_ts_namespace_declaration(&mut self, it: &TSNamespaceDeclaration<'a>) {
        self.namespace(it);
    }

    // Ambient declarations have no code.
    fn visit_ts_global_declaration(&mut self, _it: &TSGlobalDeclaration<'a>) {}

    fn visit_ts_external_module_declaration(&mut self, _it: &TSExternalModuleDeclaration<'a>) {}

    fn visit_decorator(&mut self, it: &Decorator<'a>) {
        self.decorator(it);
    }

    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        // Decorators are lowered in the enclosing body; types are not code.
        self.visit_binding_pattern(&it.pattern);
        if let Some(initializer) = &it.initializer {
            self.visit_expression(initializer);
        }
    }

    fn visit_formal_parameter_rest(&mut self, it: &FormalParameterRest<'a>) {
        self.visit_binding_rest_element(&it.rest);
    }

    // --- Module syntax ----------------------------------------------------------------------

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        self.import_declaration(it);
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        self.export_from(it);
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.export_all(it);
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        self.import_equals(it);
    }

    fn visit_export_named_declaration(&mut self, _it: &ExportNamedDeclaration<'a>) {}

    fn visit_export_declaration(&mut self, it: &ExportDeclaration<'a>) {
        self.export = Some(it.span.start);
        self.visit_declaration(&it.declaration);
        self.export = None;
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        self.export_default(it);
    }

    fn visit_ts_export_assignment(&mut self, it: &TSExportAssignment<'a>) {
        self.visit_expression(&it.expression);
    }

    // --- Expressions ------------------------------------------------------------------------

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.call_expression(it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        self.new_expression(it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        self.tagged_template(it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.import_expression(it);
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.conditional_expression(it);
    }

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        self.visit_expression(&it.left);
        self.sometimes(|lowering| lowering.visit_expression(&it.right));
    }

    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        self.visit_expression(&it.object);
        if it.optional || forms::chain_optional(&it.object) {
            self.sometimes(|lowering| lowering.visit_expression(&it.expression));
        } else {
            self.visit_expression(&it.expression);
        }
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        self.assignment(it);
    }

    fn visit_await_expression(&mut self, it: &AwaitExpression<'a>) {
        if let Some(address) = forms::value_node(&it.argument) {
            self.awaited.insert(address);
        }
        self.visit_expression(&it.argument);
    }

    fn visit_yield_expression(&mut self, it: &YieldExpression<'a>) {
        self.yield_expression(it);
    }

    // --- Types are not code -----------------------------------------------------------------

    fn visit_ts_type(&mut self, _it: &TSType<'a>) {}

    fn visit_ts_type_annotation(&mut self, _it: &TSTypeAnnotation<'a>) {}

    fn visit_ts_type_parameter_declaration(&mut self, _it: &TSTypeParameterDeclaration<'a>) {}

    fn visit_ts_type_parameter_instantiation(&mut self, _it: &TSTypeParameterInstantiation<'a>) {}

    fn visit_ts_class_implements(&mut self, _it: &TSClassImplements<'a>) {}

    fn visit_ts_index_signature(&mut self, _it: &TSIndexSignature<'a>) {}
}

impl Lowering<'_> {
    /// Lowers `inside` as code that runs only sometimes inside its expression.
    fn sometimes(&mut self, inside: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.conditional, true);
        inside(self);
        self.conditional = outer;
    }

    /// The call, construction, or conditional expression `value` is exactly, through wrappers
    /// and `await`, records that a statement assigns its value to `target`. An assignment
    /// written inside another's value doesn't rebind it: `a = b = f()` binds `a = b`.
    fn bind_value(&mut self, value: &Expression<'_>, target: &str) {
        if let Some(address) = forms::value_node(value) {
            self.binds.entry(address).or_insert_with(|| target.into());
        }
    }

    /// What a chain of plain assignments assigns, after the written target `first`: the
    /// targets joined as `a = b`, and the value at the end of the chain.
    fn chain<'e, 'a>(&self, first: &str, mut value: &'e Expression<'a>) -> (String, &'e Expression<'a>) {
        let mut targets = first.to_owned();
        while let Expression::AssignmentExpression(inner) = value
            && inner.operator == AssignmentOperator::Assign
        {
            targets.push_str(" = ");
            targets.push_str(self.text(range(inner.left.span())));
            value = &inner.right;
        }
        (targets, value)
    }
}
