//! Alien extraction: one lowering rule per form, straight to nodes keyed with `KeyBuilder`;
//! then the value table, the call facts, the imports and type names, the typed records, and the
//! binder payload.
//!
//! | Form | Node | Notes |
//! | --- | --- | --- |
//! | `(fn NAME? ...)`, `(lambda ...)`, `(method NAME ...)`, `(ctor ...)`, `(getter NAME)` | function | an unnamed one takes the name of the `const`, `let`, or `field` it initializes, else is `callback[n]` in a call's arguments and `lambda[n]` elsewhere; `(param NAME :type=T :rest)` items are parameters; `lambda` inherits `this` |
//! | `(class NAME :base=B :impl=I ...)` | class | members: `method` (`:static`, `:private`), `ctor`, `getter`, `field`; each `:base` is an `extends` link, each `:impl` an implementing one |
//! | `(call CALLEE ARG...)`, `(new ...)`, `(super CALLEE)` | call (`call`, `new`, and `call` in the construct context) | a `(star X)` argument is a spread; a literal first argument joins the key when a callback is written |
//! | `(if C T E?)` | decision `if`: part `then` (test `C`, body `T`) and part `else` | calls written in `C` are test calls |
//! | `(ret X?)` | exit `return` | `X` is its value |
//! | `(stmt X)`, `(const NAME X?)`, `(let NAME X?)` | none | a statement: its first node records it as `stmt`; a call that is exactly a `const` or `let` value binds `NAME`; `:type=C` annotates the binding |
//! | `(use NAME "spec" :export=E)`, `(import X :when-indexed?)` | none | an `imports` link from the enclosing function or module; `use` also binds `NAME`; `:when-indexed` makes it conditional, kept only when `spec` names an indexed file |
//! | `(do ...)`, `(wrap X)`, `(field NAME X?)`, `(spread ...)`, values | none | transparent |
//! | `(deco X)`, `(type ...)`, `(sig NAME)`, `(param ...)` | none | opaque: a decorator mark, skipped type syntax, an overload signature, a parameter |
//! | `(ref NAME :opts)`, `(get R NAME)`, `(key R "k")`, `(dyn R)`, `(this)`, `(base)` | none | values |
//!
//! Every function has a signature (no written return type); every `(param NAME :type=T :rest)`
//! a parameter (`this` is a `this` parameter, `:rest` a rest parameter, `:type=T` its written
//! type, which is also a `types` link from the function).

use unfold_core::key::{Callee, Child, KeyBuilder};
use unfold_core::text::{ByteRange, LineBreaks, LineIndex};
use unfold_core::typed::{Parameter, Signature, WrittenType};
use unfold_core::vocab::{CallRole, Expansion, Flag, Flags, Form, Inline, NodeKind, ParameterKind};
use unfold_core::{Node, Snapshot};
use unfold_engine::SourceFailure;
use unfold_engine::facts::{
    ArgumentFact, CallContext, CallFact, FileFacts, GraphFacts, ImportFact, TypeFacts, TypeRefFact, TypeUse,
    ValueFact, ValueShape,
};

use super::parse::{Syntax, parse};
use super::payload::{CallSite, Decl, DeclKind, Payload, Scope, ScopeKind, ValueInfo};

const FUNCTIONS: &[&str] = &["fn", "lambda", "method", "ctor", "getter"];

/// The label of a value without a static shape.
const OTHER: &str = "(…)";

/// Forms that are neither lowered nor looked into.
const OPAQUE: &[&str] = &["deco", "type", "sig", "param", "use", "import"];

/// Forms that become nodes.
const NODES: &[&str] = &[
    "fn", "lambda", "method", "ctor", "getter", "class", "call", "new", "super", "if", "ret",
];

fn failure(message: impl Into<Box<str>>) -> SourceFailure {
    SourceFailure {
        message: message.into(),
        offset: None,
    }
}

fn unquote(text: &str) -> String {
    text.trim_matches('"').to_owned()
}

/// Where a node is being lowered: under which node, and whether directly in its arguments or
/// in its test.
#[derive(Debug, Clone, Copy)]
struct Place {
    parent: u32,
    argument: bool,
    test: bool,
}

impl Place {
    const fn under(parent: u32) -> Self {
        Self {
            parent,
            argument: false,
            test: false,
        }
    }
}

struct Lowering<'a, 's> {
    syntax: &'a Syntax<'s>,
    lines: LineIndex,
    keys: KeyBuilder,
    nodes: Vec<Node>,
    /// Each syntax node's node, for the forms that are nodes.
    node_of: Vec<Option<u32>>,
    /// The statement the next node begins.
    statement: Option<ByteRange>,
}

impl<'s> Lowering<'_, 's> {
    fn head(&self, node: usize) -> &'s str {
        let found = self.syntax.node(node);
        if found.form { found.text } else { "" }
    }

    fn name_text(&self, node: usize) -> Option<&'s str> {
        self.syntax.name(node).map(|name| self.syntax.node(name).text)
    }

    fn range(&self, node: usize) -> ByteRange {
        self.syntax.node(node).range
    }

    /// The outermost `wrap` around `node` (or `node` itself), and its parent.
    fn unwrapped(&self, node: usize) -> (usize, Option<usize>) {
        let mut value = node;
        let mut parent = self.syntax.node(value).parent;
        while let Some(wrapper) = parent.filter(|&wrapper| self.head(wrapper) == "wrap") {
            value = wrapper;
            parent = self.syntax.node(value).parent;
        }
        (value, parent)
    }

    /// The name of the `const`, `let`, or `field` whose value `node` is exactly.
    fn bound_name(&self, node: usize, heads: &[&str]) -> Option<&'s str> {
        let (value, parent) = self.unwrapped(node);
        let parent = parent?;
        let initializes =
            heads.contains(&self.head(parent)) && self.syntax.items(parent).first() == Some(&value);
        initializes.then(|| self.name_text(parent)).flatten()
    }

    /// Whether `node` is exactly the value of its `ret`.
    fn returned(&self, node: usize) -> bool {
        let (value, parent) = self.unwrapped(node);
        parent.is_some_and(|parent| {
            self.head(parent) == "ret" && self.syntax.items(parent).first() == Some(&value)
        })
    }

    /// Whether an unnamed function is reached from `node` before any other node or opaque form.
    fn reaches_inline(&self, node: usize) -> bool {
        let found = self.syntax.node(node);
        if !found.form {
            return false;
        }
        match found.text {
            "fn" | "lambda" => self.function_name(node).is_none(),
            head if NODES.contains(&head) || OPAQUE.contains(&head) => false,
            _ => found.children.iter().any(|&child| self.reaches_inline(child)),
        }
    }

    fn function_name(&self, node: usize) -> Option<&'s str> {
        match self.head(node) {
            "ctor" => Some("constructor"),
            "fn" => self
                .name_text(node)
                .or_else(|| self.bound_name(node, &["const", "let", "field"])),
            "lambda" => self.bound_name(node, &["const", "let", "field"]),
            _ => self.name_text(node),
        }
    }

    /// The written callee of a call, normalized.
    fn callee(&self, node: usize) -> Callee {
        let syntax = self.syntax;
        if syntax.is_literal(node) {
            return Callee::string();
        }
        let items = syntax.items(node);
        let receiver = || {
            items
                .first()
                .map_or_else(Callee::expression, |&item| self.callee(item))
        };
        match self.head(node) {
            "ref" => Callee::name(self.name_text(node).unwrap_or("(…)")),
            "get" => receiver().member(self.name_text(node).unwrap_or("(…)")),
            "key" | "dyn" => receiver().index(),
            "call" | "new" | "super" => receiver().call(),
            "wrap" => receiver(),
            "this" => Callee::name("this"),
            "base" => Callee::name("super"),
            _ => Callee::expression(),
        }
    }

    fn push(
        &mut self,
        syntax: usize,
        place: Place,
        child: Child<'_>,
        form: Form,
        name: Option<&str>,
    ) -> Result<u32, SourceFailure> {
        let key = self
            .keys
            .push(place.parent, child)
            .map_err(|error| failure(error.to_string()))?;
        let id = self.nodes.len() as u32;
        let range = self.range(syntax);
        self.nodes.push(Node {
            key,
            parent: Some(place.parent),
            form,
            name: name.map(Into::into),
            range,
            lines: self.lines.span(range),
            head: None,
            body: None,
            doc: None,
            stmt: self.statement.take(),
            value: None,
            binds: None,
            role: None,
            r#for: None,
            flags: Flags::NONE,
            target: None,
        });
        self.node_of[syntax] = Some(id);
        Ok(id)
    }

    fn node(&mut self, id: u32) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    fn lower(&mut self, node: usize, place: Place) -> Result<(), SourceFailure> {
        let found = self.syntax.node(node);
        if !found.form {
            return Ok(());
        }
        match found.text {
            head if OPAQUE.contains(&head) => Ok(()),
            head if FUNCTIONS.contains(&head) => self.function(node, place),
            "class" => self.class(node, place),
            "call" | "new" | "super" => self.call(node, place),
            "if" => self.decision(node, place),
            "ret" => self.exit(node, place),
            "stmt" | "const" | "let" => {
                self.statement = Some(found.range);
                self.children(node, place)?;
                self.statement = None;
                Ok(())
            }
            _ => self.children(node, place),
        }
    }

    fn children(&mut self, node: usize, place: Place) -> Result<(), SourceFailure> {
        for &child in &self.syntax.node(node).children {
            self.lower(child, place)?;
        }
        Ok(())
    }

    /// A definition's head (its form's start through its last name, option, or parameter) and
    /// body (its other items).
    fn header_and_body(&self, node: usize) -> (ByteRange, Option<ByteRange>) {
        let syntax = self.syntax;
        let found = syntax.node(node);
        let mut head_end = found.range.start + 1 + found.text.len() as u32;
        let mut body: Option<ByteRange> = None;
        for &child in &found.children {
            let child_node = syntax.node(child);
            let header = (!child_node.form && !syntax.is_literal(child)) || self.head(child) == "param";
            if header {
                head_end = head_end.max(child_node.range.end);
            } else if self.head(child) != "deco" {
                let range = child_node.range;
                body = Some(body.map_or(range, |body| ByteRange {
                    start: body.start,
                    end: range.end,
                }));
            }
        }
        let head = ByteRange {
            start: found.range.start,
            end: head_end,
        };
        (head, body)
    }

    fn function(&mut self, node: usize, place: Place) -> Result<(), SourceFailure> {
        let name = self.function_name(node);
        let form = match (self.head(node), name) {
            ("fn", Some(_)) => Form::Function,
            ("method", Some(_)) => Form::Method,
            ("ctor", _) => Form::Constructor,
            ("getter", Some(_)) => Form::Getter,
            _ => Form::Lambda,
        };
        let names = name.map(|name| [name]);
        let child = match &names {
            Some(names) => Child::Definition(names),
            None if place.argument => Child::Inline(Inline::Callback),
            None => Child::Inline(Inline::Lambda),
        };
        let id = self.push(node, place, child, form, name)?;
        let (head, body) = self.header_and_body(node);
        self.node(id).head = Some(head);
        self.node(id).body = body;
        self.children(node, Place::under(id))
    }

    fn class(&mut self, node: usize, place: Place) -> Result<(), SourceFailure> {
        let name = self
            .name_text(node)
            .ok_or_else(|| failure("a class needs a name"))?;
        let id = self.push(node, place, Child::Definition(&[name]), Form::Class, Some(name))?;
        let (head, body) = self.header_and_body(node);
        self.node(id).head = Some(head);
        self.node(id).body = body;
        self.children(node, Place::under(id))
    }

    fn call(&mut self, node: usize, place: Place) -> Result<(), SourceFailure> {
        let syntax = self.syntax;
        let items = syntax.items(node);
        let (&callee_item, arguments) = items
            .split_first()
            .ok_or_else(|| failure("a call needs a callee"))?;
        let callee = self.callee(callee_item);
        let first_literal = arguments
            .first()
            .filter(|&&argument| syntax.is_literal(argument))
            .map(|&argument| unquote(syntax.node(argument).text));
        let takes_callback = arguments.iter().any(|&argument| self.reaches_inline(argument));
        let child = Child::Call {
            callee: &callee,
            first_literal: first_literal.as_deref(),
            takes_callback,
        };
        let form = if self.head(node) == "new" {
            Form::New
        } else {
            Form::Call
        };
        let id = self.push(node, place, child, form, Some(callee.text()))?;
        let binds = self.bound_name(node, &["const", "let"]).map(Into::into);
        let returned = self.returned(node);
        let callee_range = self.range(callee_item);
        let call = self.node(id);
        call.head = Some(callee_range);
        call.binds = binds;
        call.role = place.test.then_some(CallRole::Test);
        if returned {
            call.flags.insert(Flag::ExitValue);
        }
        self.lower(callee_item, Place::under(id))?;
        for &argument in arguments {
            let place = Place {
                argument: true,
                ..Place::under(id)
            };
            self.lower(argument, place)?;
        }
        Ok(())
    }

    fn decision(&mut self, node: usize, place: Place) -> Result<(), SourceFailure> {
        let items = self.syntax.items(node);
        let (test, then, otherwise) = match items.as_slice() {
            [test] => (*test, None, None),
            [test, then] => (*test, Some(*then), None),
            [test, then, otherwise] => (*test, Some(*then), Some(*otherwise)),
            _ => return Err(failure("an if takes a test, a body, and an else")),
        };
        let id = self.push(node, place, Child::Form(Form::If), Form::If, None)?;
        let test_range = self.range(test);
        let then_end = then.map_or(test_range.end, |then| self.range(then).end);
        let part = self
            .keys
            .push(id, Child::Form(Form::Then))
            .map_err(|error| failure(error.to_string()))?;
        let then_id = self.nodes.len() as u32;
        let range = ByteRange {
            start: test_range.start,
            end: then_end,
        };
        self.nodes.push(Node {
            key: part,
            parent: Some(id),
            form: Form::Then,
            name: None,
            range,
            lines: self.lines.span(range),
            head: Some(test_range),
            body: then.map(|then| self.range(then)),
            doc: None,
            stmt: None,
            value: None,
            binds: None,
            role: None,
            r#for: None,
            flags: Flags::NONE,
            target: None,
        });
        let test_place = Place {
            test: true,
            ..Place::under(then_id)
        };
        self.lower(test, test_place)?;
        if let Some(then) = then {
            self.lower(then, Place::under(then_id))?;
        }
        if let Some(otherwise) = otherwise {
            let else_id = self.push(
                otherwise,
                Place::under(id),
                Child::Form(Form::Else),
                Form::Else,
                None,
            )?;
            // The part was registered under its item; the item itself is lowered inside it.
            self.node_of[otherwise] = None;
            let range = self.range(otherwise);
            self.node(else_id).body = Some(range);
            self.lower(otherwise, Place::under(else_id))?;
        }
        Ok(())
    }

    fn exit(&mut self, node: usize, place: Place) -> Result<(), SourceFailure> {
        let range = self.range(node);
        let statement = self.statement.take().unwrap_or(range);
        let id = self.push(node, place, Child::Form(Form::Return), Form::Return, None)?;
        let value = self.syntax.items(node).first().copied();
        let value_range = value.map(|value| self.range(value));
        let exit = self.node(id);
        exit.stmt = Some(statement);
        exit.value = value_range;
        if let Some(value) = value {
            self.lower(value, Place::under(id))?;
        }
        Ok(())
    }

    /// The nearest node at or above syntax node `node` that `fits`, or the module.
    fn enclosing(&self, node: usize, fits: impl Fn(&Node) -> bool) -> u32 {
        let mut current = Some(node);
        while let Some(at) = current {
            if let Some(id) = self.node_of[at].filter(|&id| fits(&self.nodes[id as usize])) {
                return id;
            }
            current = self.syntax.node(at).parent;
        }
        0
    }
}

/// The range and text of each `:key=value` option of `node`, in written order.
fn option_values(syntax: &Syntax<'_>, node: usize, key: &str) -> Vec<(ByteRange, String)> {
    let prefix = format!(":{key}=");
    syntax
        .node(node)
        .children
        .iter()
        .copied()
        .filter(|&child| syntax.is_option(child) && syntax.node(child).text.starts_with(&prefix))
        .filter_map(|child| {
            let token = syntax.node(child);
            let value = &token.text[prefix.len()..];
            let range = ByteRange::new(token.range.end - value.len() as u32, token.range.end).ok()?;
            (!value.is_empty()).then(|| (range, value.to_owned()))
        })
        .collect()
}

/// Extracts one alien file's facts.
pub fn extract(snapshot: &Snapshot) -> Result<FileFacts, SourceFailure> {
    let syntax = parse(snapshot.text()).map_err(|error| SourceFailure {
        message: error.message.into(),
        offset: Some(error.offset),
    })?;
    let count = syntax.nodes.len();
    let lines = snapshot.line_index(LineBreaks::EcmaScript);
    let keys = KeyBuilder::new(snapshot.path());
    let module = Node {
        key: keys.keys()[0].clone(),
        parent: None,
        form: Form::Module,
        name: None,
        range: syntax.node(0).range,
        lines: lines.span(syntax.node(0).range),
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
    let mut node_of = vec![None; count];
    node_of[0] = Some(0);
    let mut lowering = Lowering {
        syntax: &syntax,
        lines,
        keys,
        nodes: vec![module],
        node_of,
        statement: None,
    };
    lowering.children(0, Place::under(0))?;

    let (mut decls, scopes, inits, outer, inner) = bind(&lowering);
    let (mut values, mut infos, value_of) = record_values(&lowering, &outer);
    for (decl, init) in inits {
        if let DeclKind::Variable { init: slot, .. } | DeclKind::Field { init: slot, .. } =
            &mut decls[decl].kind
        {
            *slot = value_of[init];
        }
    }

    // One call fact per call node, in node order.
    let mut call_nodes: Vec<(u32, usize)> = (0..count)
        .filter_map(|node| {
            let id = lowering.node_of[node]?;
            matches!(lowering.head(node), "call" | "new" | "super").then_some((id, node))
        })
        .collect();
    call_nodes.sort_unstable();
    let mut calls = Vec::new();
    for (id, node) in call_nodes {
        let items = syntax.items(node);
        let callee_item = items[0];
        let callee = match value_of[callee_item] {
            Some(callee) => callee,
            None => {
                values.push(ValueFact::new(
                    ValueShape::Other,
                    OTHER,
                    syntax.node(callee_item).range,
                ));
                infos.push(ValueInfo::Other);
                values.len() as u32 - 1
            }
        };
        let arguments = items[1..]
            .iter()
            .map(|&item| {
                let spread = lowering.head(item) == "star";
                let value =
                    value_of[item].filter(|&value| !spread && values[value as usize].shape.is_reference());
                ArgumentFact {
                    range: syntax.node(item).range,
                    value,
                    expand: spread.then_some(Expansion::Spread),
                    keyword: None,
                }
            })
            .collect();
        calls.push(CallFact {
            node: id,
            callee,
            context: if lowering.head(node) == "call" {
                CallContext::Call
            } else {
                CallContext::Construct
            },
            arguments,
        });
    }

    let imports = imports(&lowering)?;
    let (type_refs, type_names) = type_names(&lowering, &outer, &inner);
    let types = typed(&lowering);
    let payload = Payload {
        decls,
        scopes,
        values: infos,
        call_sites: call_sites(&syntax),
        types: type_names,
    };
    let graph = GraphFacts::new(lowering.nodes, values, calls, imports, type_refs, types)
        .map_err(|error| failure(error.to_string()))?;
    let payload = postcard::to_allocvec(&payload).map_err(|error| failure(error.to_string()))?;
    Ok(FileFacts::new(graph, payload))
}

/// What binding records: declarations and scopes in preorder, each variable's and field's
/// declaration with its initializer's syntax node, and each syntax node's outer and inner scope.
type Bound = (Vec<Decl>, Vec<Scope>, Vec<(usize, usize)>, Vec<u32>, Vec<u32>);

fn bind(lowering: &Lowering<'_, '_>) -> Bound {
    let syntax = lowering.syntax;
    let count = syntax.nodes.len();
    let mut scopes = vec![Scope {
        parent: None,
        kind: ScopeKind::File,
        decls: Vec::new(),
    }];
    let mut decls: Vec<Decl> = Vec::new();
    let mut inits: Vec<(usize, usize)> = Vec::new();
    let mut outer = vec![0u32; count];
    let mut inner = vec![0u32; count];
    let mut class_decl: Vec<Option<u32>> = vec![None; count];
    let mut stack = vec![0usize];
    while let Some(node) = stack.pop() {
        let found = syntax.node(node);
        let scope = found.parent.map_or(0, |parent| inner[parent]);
        outer[node] = scope;
        inner[node] = scope;
        let head = lowering.head(node);
        let name = lowering.name_text(node).unwrap_or_default().to_owned();
        let id = lowering.node_of[node].unwrap_or(0);
        let owner = found.parent.and_then(|parent| class_decl[parent]);
        let mut declare = |decl: Decl, bind: Option<u32>| {
            let index = decls.len() as u32;
            decls.push(decl);
            if let Some(bind) = bind {
                scopes[bind as usize].decls.push(index);
            }
            index
        };
        if found.form && node != 0 {
            match head {
                "fn" if !name.is_empty() => {
                    declare(
                        Decl {
                            name,
                            scope,
                            kind: DeclKind::Function { node: id },
                        },
                        Some(scope),
                    );
                }
                "class" => {
                    let bases = syntax
                        .options(node)
                        .into_iter()
                        .filter(|(key, _)| *key == "base")
                        .filter_map(|(_, value)| value.map(str::to_owned))
                        .collect();
                    let decorated = syntax
                        .items(node)
                        .iter()
                        .any(|&item| lowering.head(item) == "deco");
                    let kind = DeclKind::Class {
                        node: id,
                        ctor: None,
                        bases,
                        members: Vec::new(),
                        decorated,
                    };
                    class_decl[node] = Some(declare(Decl { name, scope, kind }, Some(scope)));
                }
                "method" | "ctor" | "getter" | "field" => {
                    if let Some(owner) = owner {
                        let kind = match head {
                            "method" => DeclKind::Method {
                                node: id,
                                owner,
                                is_static: syntax.option(node, "static").is_some(),
                                is_private: syntax.option(node, "private").is_some(),
                            },
                            "ctor" => DeclKind::Ctor { node: id, owner },
                            "getter" => DeclKind::Getter { node: id, owner },
                            _ => DeclKind::Field { owner, init: None },
                        };
                        let name = if head == "ctor" {
                            "constructor".to_owned()
                        } else {
                            name
                        };
                        let member = declare(Decl { name, scope, kind }, None);
                        if head == "field"
                            && let Some(&init) = syntax.items(node).first()
                        {
                            inits.push((member as usize, init));
                        }
                        if let DeclKind::Class { ctor, members, .. } = &mut decls[owner as usize].kind {
                            members.push(member);
                            if head == "ctor" {
                                *ctor = Some(id);
                            }
                        }
                    }
                }
                "const" | "let" => {
                    let kind = DeclKind::Variable {
                        mutable: head == "let",
                        init: None,
                        ty: syntax.option(node, "type").map(str::to_owned),
                    };
                    let variable = declare(Decl { name, scope, kind }, Some(scope));
                    if let Some(&init) = syntax.items(node).first() {
                        inits.push((variable as usize, init));
                    }
                }
                "sig" => {
                    declare(
                        Decl {
                            name,
                            scope,
                            kind: DeclKind::Signature,
                        },
                        Some(scope),
                    );
                }
                "param" => {
                    // Parameters live in their function's own scope.
                    let function = found.parent.unwrap_or(0);
                    let bind = inner[function];
                    let ty = syntax.option(node, "type").map(str::to_owned);
                    declare(
                        Decl {
                            name,
                            scope: bind,
                            kind: DeclKind::Param { ty },
                        },
                        Some(bind),
                    );
                }
                "use" => {
                    let spec = syntax
                        .items(node)
                        .into_iter()
                        .find(|&item| syntax.is_literal(item))
                        .map(|item| unquote(syntax.node(item).text))
                        .unwrap_or_default();
                    let export = syntax
                        .option(node, "export")
                        .map_or_else(|| name.clone(), str::to_owned);
                    declare(
                        Decl {
                            name,
                            scope,
                            kind: DeclKind::Use { spec, export },
                        },
                        Some(scope),
                    );
                }
                _ => {}
            }
            if FUNCTIONS.contains(&head) {
                let class = match head {
                    "method" | "ctor" | "getter" => owner,
                    _ => None,
                };
                scopes.push(Scope {
                    parent: Some(scope),
                    kind: ScopeKind::Function {
                        arrow: head == "lambda",
                        class,
                    },
                    decls: Vec::new(),
                });
                inner[node] = scopes.len() as u32 - 1;
            }
        }
        stack.extend(found.children.iter().rev().copied());
    }
    (decls, scopes, inits, outer, inner)
}

/// The value table and its payload, in postorder so receivers precede their accesses, with each
/// syntax node's value.
fn record_values(
    lowering: &Lowering<'_, '_>,
    outer: &[u32],
) -> (Vec<ValueFact>, Vec<ValueInfo>, Vec<Option<u32>>) {
    let syntax = lowering.syntax;
    let count = syntax.nodes.len();
    let mut values: Vec<ValueFact> = Vec::new();
    let mut infos: Vec<ValueInfo> = Vec::new();
    let mut value_of: Vec<Option<u32>> = vec![None; count];
    let mut order = Vec::with_capacity(count);
    let mut stack = vec![(0usize, false)];
    while let Some((node, done)) = stack.pop() {
        if done {
            order.push(node);
            continue;
        }
        stack.push((node, true));
        stack.extend(
            syntax
                .node(node)
                .children
                .iter()
                .rev()
                .map(|&child| (child, false)),
        );
    }
    for node in order {
        let items = syntax.items(node);
        let item_value = |index: usize| items.get(index).and_then(|&item| value_of[item]);
        let label_of = |value: Option<u32>| {
            value.map_or(OTHER.to_owned(), |value| values[value as usize].label.to_string())
        };
        let range = syntax.node(node).range;
        let scope = outer[node];
        let entry: Option<(ValueFact, ValueInfo)> = if syntax.is_literal(node) {
            Some((
                ValueFact::new(ValueShape::Other, OTHER, range),
                ValueInfo::Literal(unquote(syntax.node(node).text)),
            ))
        } else if !syntax.node(node).form || node == 0 {
            None
        } else {
            let name = lowering.name_text(node).unwrap_or_default().to_owned();
            match lowering.head(node) {
                "ref" => {
                    let options = syntax
                        .options(node)
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), value.map(str::to_owned)))
                        .collect();
                    Some((
                        ValueFact::new(ValueShape::Name, &name, range),
                        ValueInfo::Ref { name, scope, options },
                    ))
                }
                "get" => item_value(0).map(|receiver| {
                    let label = format!("{}.{name}", label_of(Some(receiver)));
                    let mut fact = ValueFact::new(ValueShape::Member, &label, range);
                    fact.receiver = Some(receiver);
                    fact.member = Some(name.as_str().into());
                    let external = syntax.option(node, "ext").map(str::to_owned);
                    (
                        fact,
                        ValueInfo::Get {
                            receiver,
                            name,
                            external,
                        },
                    )
                }),
                "key" => item_value(0).map(|receiver| {
                    let key = items.get(1).map_or("", |&item| syntax.node(item).text);
                    let label = format!("{}[{key}]", label_of(Some(receiver)));
                    let mut fact = ValueFact::new(ValueShape::Member, &label, range);
                    fact.receiver = Some(receiver);
                    (
                        fact,
                        ValueInfo::Key {
                            receiver,
                            name: unquote(key),
                        },
                    )
                }),
                "dyn" => item_value(0).map(|receiver| {
                    let label = format!("{}[…]", label_of(Some(receiver)));
                    let mut fact = ValueFact::new(ValueShape::Other, &label, range);
                    fact.receiver = Some(receiver);
                    (fact, ValueInfo::Dynamic)
                }),
                "this" => Some((
                    ValueFact::new(ValueShape::Other, "this", range),
                    ValueInfo::This { scope },
                )),
                "base" => Some((
                    ValueFact::new(ValueShape::Other, "super", range),
                    ValueInfo::Base { scope },
                )),
                "call" | "super" => {
                    let callee = item_value(0);
                    let label = format!("{}()", label_of(callee));
                    Some((
                        ValueFact::new(ValueShape::Other, &label, range),
                        ValueInfo::Call { callee },
                    ))
                }
                "new" => {
                    let callee = item_value(0);
                    Some((
                        ValueFact::new(ValueShape::Other, &label_of(callee), range),
                        ValueInfo::Call { callee },
                    ))
                }
                "fn" | "lambda" => lowering.node_of[node].map(|id| {
                    (
                        ValueFact::new(ValueShape::Other, OTHER, range),
                        ValueInfo::Inline { node: id },
                    )
                }),
                "wrap" | "star" => {
                    value_of[node] = item_value(0);
                    None
                }
                "spread" => Some((ValueFact::new(ValueShape::Other, OTHER, range), ValueInfo::Other)),
                _ => None,
            }
        };
        if let Some((fact, info)) = entry {
            value_of[node] = Some(values.len() as u32);
            values.push(fact);
            infos.push(info);
        }
    }
    (values, infos, value_of)
}

/// The `imports` links before linking.
fn imports(lowering: &Lowering<'_, '_>) -> Result<Vec<ImportFact>, SourceFailure> {
    let syntax = lowering.syntax;
    let mut imports = Vec::new();
    let mut stack = vec![0usize];
    while let Some(node) = stack.pop() {
        let head = lowering.head(node);
        if matches!(head, "deco" | "type" | "sig" | "param") {
            continue;
        }
        if matches!(head, "use" | "import") {
            let items = syntax.items(node);
            let literal = items.iter().copied().find(|&item| syntax.is_literal(item));
            let (range, specifier) = match (head, literal, items.first()) {
                (_, Some(literal), _) => (
                    syntax.node(literal).range,
                    Some(unquote(syntax.node(literal).text).into()),
                ),
                ("import", None, Some(&expression)) => (syntax.node(expression).range, None),
                _ => return Err(failure("an import needs a specifier")),
            };
            let from = lowering.enclosing(node, |found| {
                matches!(found.kind(), NodeKind::Module | NodeKind::Function)
            });
            imports.push(ImportFact {
                from,
                range,
                specifier,
                conditional: syntax.option(node, "when-indexed").is_some(),
            });
        }
        stack.extend(syntax.node(node).children.iter().rev().copied());
    }
    Ok(imports)
}

/// The written type names (each `:type` of a parameter or binding, and each `:base` and
/// `:impl` of a class), and each one's scope and text for the payload.
fn type_names(
    lowering: &Lowering<'_, '_>,
    outer: &[u32],
    inner: &[u32],
) -> (Vec<TypeRefFact>, Vec<(u32, String)>) {
    let syntax = lowering.syntax;
    let mut references = Vec::new();
    let mut names = Vec::new();
    for (node, &scope) in outer.iter().enumerate().skip(1) {
        let skipped = std::iter::successors(Some(node), |&at| syntax.node(at).parent)
            .any(|at| matches!(lowering.head(at), "type" | "deco" | "sig"));
        if skipped {
            continue;
        }
        let mut add = |from: u32, usage: TypeUse, lookup: u32, (range, text): (ByteRange, String)| {
            references.push(TypeRefFact::new(from, range, &text, false, usage));
            names.push((lookup, text));
        };
        match lowering.head(node) {
            "param" => {
                let Some(function) = syntax.node(node).parent else {
                    continue;
                };
                let Some(from) = lowering.node_of[function] else {
                    continue;
                };
                for written in option_values(syntax, node, "type") {
                    add(from, TypeUse::Annotation, inner[function], written);
                }
            }
            "const" | "let" => {
                let from = lowering.enclosing(node, |found| found.kind().is_definition());
                for written in option_values(syntax, node, "type") {
                    add(from, TypeUse::Annotation, scope, written);
                }
            }
            "class" => {
                let Some(from) = lowering.node_of[node] else {
                    continue;
                };
                for written in option_values(syntax, node, "base") {
                    add(from, TypeUse::Extends, scope, written);
                }
                for written in option_values(syntax, node, "impl") {
                    add(from, TypeUse::Implements, scope, written);
                }
            }
            _ => {}
        }
    }
    (references, names)
}

/// One signature per function node, and one parameter per `(param ...)` of each.
fn typed(lowering: &Lowering<'_, '_>) -> TypeFacts {
    let syntax = lowering.syntax;
    let mut signatures = Vec::new();
    let mut parameters = Vec::new();
    for node in 1..syntax.nodes.len() {
        let Some(id) = lowering.node_of[node] else {
            continue;
        };
        if !FUNCTIONS.contains(&lowering.head(node)) {
            continue;
        }
        signatures.push(Signature {
            node: id,
            returns: None,
        });
        let params = syntax
            .node(node)
            .children
            .iter()
            .copied()
            .filter(|&child| lowering.head(child) == "param");
        for (index, param) in params.enumerate() {
            let name = lowering.name_text(param).map(str::to_owned);
            let kind = match (name.as_deref(), syntax.option(param, "rest")) {
                (Some("this"), _) => ParameterKind::This,
                (_, Some(_)) => ParameterKind::Rest,
                _ => ParameterKind::Positional,
            };
            let ty = option_values(syntax, param, "type")
                .into_iter()
                .next()
                .map(|(range, text)| WrittenType {
                    text: text.into(),
                    range,
                });
            parameters.push(Parameter {
                node: id,
                index: index as u32,
                range: syntax.node(param).range,
                kind,
                name: name.map(Into::into),
                optional: false,
                default: false,
                ty,
            });
        }
    }
    TypeFacts::new(signatures, parameters, Vec::new())
}

/// Every call, construction, and `super` form outside opaque syntax: what the census checks the
/// call nodes against, found from the parser alone.
fn call_sites(syntax: &Syntax<'_>) -> Vec<CallSite> {
    let mut sites = Vec::new();
    let mut stack = vec![0usize];
    while let Some(node) = stack.pop() {
        let found = syntax.node(node);
        if !found.form || (node != 0 && OPAQUE.contains(&found.text)) {
            continue;
        }
        let form = match found.text {
            "new" => Some(Some(Form::New)),
            "super" => Some(Some(Form::Call)),
            "call" => Some(None),
            _ => None,
        };
        if let Some(form) = form.filter(|_| node != 0) {
            sites.push(CallSite {
                range: found.range,
                form,
            });
        }
        stack.extend(found.children.iter().rev().copied());
    }
    sites
}
