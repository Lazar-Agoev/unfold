//! Alien extraction: the graph mapping that drives the shared selection, then the value table,
//! occurrences, and the private payload.
//!
//! | Form | Block | Notes |
//! | --- | --- | --- |
//! | `(fn NAME? ...)`, `(lambda ...)` | `function` | `(param NAME)` items are parameters; `lambda` inherits `this` |
//! | `(class NAME :base=B ...)`, `(klass ...)` | `class` | members: `method` (`:static`, `:private`), `ctor`, `getter`, `field` |
//! | `(const NAME init?)`, `(let NAME init?)` | `variable` | `:type=C` annotates an instance of class `C` |
//! | `(call CALLEE ARG...)`, `(new ...)`, `(super CALLEE)` | `call`, `new`, `call` (construct context) | `(star X)` arguments never supply references |
//! | `(use NAME "spec" :export=E)`, `(import "spec"? :when-indexed?)` | `import` | `use` also binds `NAME`; `:when-indexed` makes the block conditional: kept only when `spec` names an indexed file |
//! | `(if C T E?)`, `(stmt X)`, `(ret X?)` | `if`, `expression_statement`, `return` | |
//! | `(do ...)`, `(wrap X)`, `(deco X)`, `(type ...)` | none | a brace block, a transparent wrapper, a decorator, skipped type syntax |
//! | `(ref NAME :opts)`, `(get R NAME)`, `(key R "k")`, `(dyn R)`, `(this)`, `(base)` | none | values |
//!
//! Typed facts: every function-like block has a signature (no written return type); every
//! `(param NAME :type=T :rest)` a parameter (`this` is a `this` parameter, `:rest` a rest
//! parameter, `:type=T` its written type); every call a result (`stmt` discards it, `ret`
//! returns it, a `const`/`let` initializer binds it, a call argument passes it, anything else is
//! `other`); a `(star X)` argument is a spread; and each `:type=T` of a parameter or variable is a
//! type name, looked up like a value name in the `type` context.

use unfold_core::Snapshot;
use unfold_core::text::ByteRange;
use unfold_core::typed::{CallResult, Parameter, Signature, WrittenType};
use unfold_core::vocab::{BlockKind, EdgeKind, Expansion, ParameterKind, ResultBinding, Role, SlotName};
use unfold_engine::SourceFailure;
use unfold_engine::edges::text;
use unfold_engine::facts::{
    ArgumentFact, CallContext, CallFact, FileFacts, GraphFacts, ImportFact, TypeFacts, TypeRefFact,
    ValueFact, ValueShape,
};
use unfold_engine::select::{self, MappedSlot, Mapping, Saved, Tree};

use super::parse::{Syntax, parse};
use super::payload::{CallSite, Decl, DeclKind, Payload, Scope, ScopeKind, ValueInfo};

/// The alien child-slot labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Body,
    Param,
    Deco,
    Init,
    Callee,
    Arg,
    Cond,
    Then,
    Else,
    Expr,
    Inner,
    Recv,
    Item,
}

const FUNCTIONS: &[&str] = &["fn", "lambda", "method", "ctor", "getter"];

struct Alien<'a, 's> {
    syntax: &'a Syntax<'s>,
}

impl Alien<'_, '_> {
    fn head(&self, node: usize) -> &str {
        let found = self.syntax.node(node);
        if found.form { found.text } else { "" }
    }

    fn name_text(&self, node: usize) -> Option<&str> {
        self.syntax.name(node).map(|name| self.syntax.node(name).text)
    }

    /// A `lambda`, `klass`, or anonymous `fn` takes the name of the variable or field it
    /// initializes, looking through `wrap`.
    fn inferred_name(&self, node: usize) -> Option<&str> {
        let mut value = node;
        let mut parent = self.syntax.node(value).parent?;
        while self.head(parent) == "wrap" {
            value = parent;
            parent = self.syntax.node(value).parent?;
        }
        let initializes = matches!(self.head(parent), "const" | "let" | "field")
            && self.syntax.items(parent).first() == Some(&value);
        initializes.then(|| self.name_text(parent)).flatten()
    }
}

impl Mapping for Alien<'_, '_> {
    type Node = usize;
    type Slot = Slot;

    fn range(&self, node: usize) -> ByteRange {
        self.syntax.node(node).range
    }

    fn skip(&self, node: usize) -> bool {
        self.head(node) == "type"
    }

    fn decorator(&self, node: usize) -> bool {
        self.head(node) == "deco"
    }

    fn kind(&self, node: usize) -> Option<BlockKind> {
        if node == 0 {
            return Some(BlockKind::File);
        }
        Some(match self.head(node) {
            "fn" | "lambda" => BlockKind::Function,
            "class" | "klass" => BlockKind::Class,
            "method" => BlockKind::Method,
            "ctor" => BlockKind::Constructor,
            "getter" => BlockKind::Getter,
            "const" | "let" => BlockKind::Variable,
            "call" | "super" => BlockKind::Call,
            "new" => BlockKind::New,
            "use" | "import" => BlockKind::Import,
            "if" => BlockKind::If,
            "stmt" => BlockKind::ExpressionStatement,
            "ret" => BlockKind::Return,
            _ => return None,
        })
    }

    fn name(&self, node: usize, kind: BlockKind) -> Option<Box<str>> {
        match (kind, self.head(node)) {
            (BlockKind::Import, _) => None,
            (BlockKind::Constructor, _) => Some("constructor".into()),
            (_, "lambda" | "klass") => self.inferred_name(node).map(Into::into),
            (_, "fn") => self
                .name_text(node)
                .or_else(|| self.inferred_name(node))
                .map(Into::into),
            _ => self.name_text(node).map(Into::into),
        }
    }

    fn role(&self, _parent: Saved<usize>, first_slot: Option<Slot>, through_decorator: bool) -> Option<Role> {
        if through_decorator {
            return Some(Role::Decorator);
        }
        Some(match first_slot? {
            Slot::Body => Role::Body,
            Slot::Param => Role::Parameter,
            Slot::Init => Role::Initializer,
            Slot::Callee => Role::Callee,
            Slot::Arg => Role::Argument,
            Slot::Cond => Role::Condition,
            Slot::Then => Role::Then,
            Slot::Else => Role::Else,
            Slot::Expr => Role::Expression,
            Slot::Deco | Slot::Inner | Slot::Recv | Slot::Item => return None,
        })
    }

    fn slots(&self, node: usize, kind: BlockKind, out: &mut Vec<MappedSlot>) {
        let syntax = self.syntax;
        let mut add = |slot, index, range| out.push(MappedSlot { slot, index, range });
        let items = syntax.items(node);
        let range = |item: usize| syntax.node(item).range;
        let name = syntax.name(node).map(range);
        match kind {
            kind if kind.is_function_like() => {
                if let Some(name) = name.filter(|_| kind != BlockKind::Constructor) {
                    add(SlotName::Name, None, name);
                }
                let mut body = Vec::new();
                let mut parameters = 0;
                for &item in &items {
                    match self.head(item) {
                        "param" => {
                            add(SlotName::Parameter, Some(parameters), range(item));
                            parameters += 1;
                        }
                        "deco" => {}
                        _ => body.push(range(item)),
                    }
                }
                if let (Some(first), Some(last)) = (body.first(), body.last()) {
                    add(
                        SlotName::Body,
                        None,
                        ByteRange::new(first.start, last.end).expect("ordered items"),
                    );
                }
            }
            BlockKind::Class => {
                if let Some(name) = name {
                    add(SlotName::Name, None, name);
                }
                let base =
                    syntax.node(node).children.iter().copied().find(|&child| {
                        syntax.is_option(child) && syntax.node(child).text.starts_with(":base=")
                    });
                if let Some(base) = base {
                    add(SlotName::Heritage, None, range(base));
                }
            }
            BlockKind::Variable => {
                if let Some(name) = name {
                    add(SlotName::Name, None, name);
                }
                if let Some(&init) = items.first() {
                    add(SlotName::Initializer, None, range(init));
                }
            }
            BlockKind::Call | BlockKind::New => {
                if let Some(&callee) = items.first() {
                    add(SlotName::Callee, None, range(callee));
                }
                for (index, &argument) in items.iter().skip(1).enumerate() {
                    add(SlotName::Argument, Some(index as u32), range(argument));
                }
            }
            BlockKind::If => {
                for (slot, &item) in [SlotName::Condition, SlotName::Then, SlotName::Else]
                    .into_iter()
                    .zip(&items)
                {
                    add(slot, None, range(item));
                }
            }
            BlockKind::ExpressionStatement | BlockKind::Return => {
                if let Some(&expression) = items.first() {
                    add(SlotName::Expression, None, range(expression));
                }
            }
            BlockKind::Import => {
                if let Some(&specifier) = items.iter().find(|&&item| syntax.is_literal(item)) {
                    add(SlotName::Specifier, None, range(specifier));
                }
            }
            _ => {}
        }
    }
}

impl Tree for Alien<'_, '_> {
    fn children(&self, node: usize, out: &mut Vec<(Slot, usize)>) {
        let head = if node == 0 { "file" } else { self.head(node) };
        let items = self.syntax.items(node);
        for (index, &item) in items.iter().enumerate() {
            let slot = match head {
                "file" => Slot::Body,
                head if FUNCTIONS.contains(&head) => match self.head(item) {
                    "param" => Slot::Param,
                    "deco" => Slot::Deco,
                    _ => Slot::Body,
                },
                "class" | "klass" => match self.head(item) {
                    "deco" => Slot::Deco,
                    _ => Slot::Body,
                },
                "const" | "let" | "field" => Slot::Init,
                "call" | "new" | "super" if index == 0 => Slot::Callee,
                "call" | "new" | "super" => Slot::Arg,
                "if" => [Slot::Cond, Slot::Then, Slot::Else][index.min(2)],
                "stmt" | "ret" => Slot::Expr,
                "get" | "key" | "dyn" => Slot::Recv,
                "do" => Slot::Item,
                _ => Slot::Inner,
            };
            out.push((slot, item));
        }
    }
}

fn unquote(text: &str) -> String {
    text.trim_matches('"').to_owned()
}

/// Extracts one alien file's facts.
pub fn extract(snapshot: &Snapshot) -> Result<FileFacts, SourceFailure> {
    let syntax = parse(snapshot.text()).map_err(|error| SourceFailure {
        message: error.message.into(),
        offset: Some(error.offset),
    })?;
    let alien = Alien { syntax: &syntax };
    let lines = snapshot.line_index();
    let count = syntax.nodes.len();
    let mut block_of: Vec<Option<u32>> = vec![None; count];
    let selection = select::walk(&alien, snapshot.path(), &lines, 0, |node, local| {
        block_of[node] = Some(local);
    })
    .map_err(|error| SourceFailure {
        message: error.to_string().into(),
        offset: None,
    })?;

    // Scopes and declarations, in preorder.
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
    // Nodes inside skipped `type` syntax have no runtime calls, so the census leaves them out.
    let mut skipped = vec![false; count];
    let mut stack = vec![0usize];
    while let Some(node) = stack.pop() {
        let found = syntax.node(node);
        let scope = found.parent.map_or(0, |parent| inner[parent]);
        outer[node] = scope;
        inner[node] = scope;
        let head = alien.head(node);
        skipped[node] = head == "type" || found.parent.is_some_and(|parent| skipped[parent]);
        let name = alien.name_text(node).unwrap_or_default().to_owned();
        let block = block_of[node].unwrap_or(0);
        let owner = found.parent.and_then(|parent| class_decl[parent]);
        let mut declare = |decl: Decl, bind: Option<u32>| {
            let id = decls.len() as u32;
            decls.push(decl);
            if let Some(bind) = bind {
                scopes[bind as usize].decls.push(id);
            }
            id
        };
        if found.form && node != 0 {
            match head {
                "fn" if !name.is_empty() => {
                    declare(
                        Decl {
                            name,
                            scope,
                            kind: DeclKind::Function { block },
                        },
                        Some(scope),
                    );
                }
                "class" | "klass" => {
                    let bases = syntax
                        .options(node)
                        .into_iter()
                        .filter(|(key, _)| *key == "base")
                        .filter_map(|(_, value)| value.map(str::to_owned))
                        .collect();
                    let bind = (head == "class").then_some(scope);
                    let decorated = syntax.items(node).iter().any(|&item| alien.head(item) == "deco");
                    let kind = DeclKind::Class {
                        block,
                        ctor: None,
                        bases,
                        members: Vec::new(),
                        decorated,
                    };
                    class_decl[node] = Some(declare(Decl { name, scope, kind }, bind));
                }
                "method" | "ctor" | "getter" | "field" => {
                    if let Some(owner) = owner {
                        let kind = match head {
                            "method" => DeclKind::Method {
                                block,
                                owner,
                                is_static: syntax.option(node, "static").is_some(),
                                is_private: syntax.option(node, "private").is_some(),
                            },
                            "ctor" => DeclKind::Ctor { block, owner },
                            "getter" => DeclKind::Getter { block, owner },
                            _ => DeclKind::Field { owner, init: None },
                        };
                        let name = if head == "ctor" {
                            "constructor".to_owned()
                        } else {
                            name
                        };
                        let id = declare(Decl { name, scope, kind }, None);
                        if head == "field"
                            && let Some(&init) = syntax.items(node).first()
                        {
                            inits.push((id as usize, init));
                        }
                        if let DeclKind::Class { ctor, members, .. } = &mut decls[owner as usize].kind {
                            members.push(id);
                            if head == "ctor" {
                                *ctor = Some(block);
                            }
                        }
                    }
                }
                "const" | "let" => {
                    let kind = DeclKind::Variable {
                        block,
                        mutable: head == "let",
                        init: None,
                        ty: syntax.option(node, "type").map(str::to_owned),
                    };
                    let id = declare(Decl { name, scope, kind }, Some(scope));
                    if let Some(&init) = syntax.items(node).first() {
                        inits.push((id as usize, init));
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
                    // Parameters live in their callable's own scope; an explicit `this`
                    // parameter takes no runtime position (C-4).
                    let callable = found.parent.unwrap_or(0);
                    let earlier = syntax
                        .node(callable)
                        .children
                        .iter()
                        .take_while(|&&child| child != node);
                    let position = earlier
                        .filter(|&&child| {
                            alien.head(child) == "param" && alien.name_text(child) != Some("this")
                        })
                        .count() as u32;
                    let position = (name != "this").then_some(position);
                    let bind = inner[callable];
                    let owner = block_of[callable].unwrap_or(0);
                    let ty = syntax.option(node, "type").map(str::to_owned);
                    declare(
                        Decl {
                            name,
                            scope: bind,
                            kind: DeclKind::Param { owner, position, ty },
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

    // Values, in postorder so receivers precede their accesses.
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
            value.map_or(text::OTHER.to_owned(), |value| {
                values[value as usize].label.to_string()
            })
        };
        let scope = outer[node];
        let entry: Option<(ValueFact, ValueInfo)> = if syntax.is_literal(node) {
            Some((
                ValueFact::new(ValueShape::Other, text::OTHER),
                ValueInfo::Literal(unquote(syntax.node(node).text)),
            ))
        } else if !syntax.node(node).form || node == 0 {
            None
        } else {
            let name = alien.name_text(node).unwrap_or_default().to_owned();
            match alien.head(node) {
                "ref" => {
                    let options = syntax
                        .options(node)
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), value.map(str::to_owned)))
                        .collect();
                    Some((
                        ValueFact::new(ValueShape::Name, &name),
                        ValueInfo::Ref { name, scope, options },
                    ))
                }
                "get" => item_value(0).map(|receiver| {
                    let mut fact = ValueFact::new(
                        ValueShape::Member,
                        &text::member(&label_of(Some(receiver)), &name),
                    );
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
                    let mut fact = ValueFact::new(
                        ValueShape::Member,
                        &text::literal_key(&label_of(Some(receiver)), key),
                    );
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
                    let mut fact =
                        ValueFact::new(ValueShape::Other, &text::computed_key(&label_of(Some(receiver))));
                    fact.receiver = Some(receiver);
                    (fact, ValueInfo::Dynamic)
                }),
                "this" => Some((
                    ValueFact::new(ValueShape::Other, "this"),
                    ValueInfo::This { scope },
                )),
                "base" => Some((
                    ValueFact::new(ValueShape::Other, "super"),
                    ValueInfo::Base { scope },
                )),
                "call" | "super" => {
                    let callee = item_value(0);
                    let label = text::call_result(&label_of(callee));
                    Some((
                        ValueFact::new(ValueShape::CallResult, &label),
                        ValueInfo::Call { callee },
                    ))
                }
                "new" => {
                    let callee = item_value(0);
                    Some((
                        ValueFact::new(ValueShape::Other, &label_of(callee)),
                        ValueInfo::Call { callee },
                    ))
                }
                "fn" | "lambda" | "klass" | "class" => block_of[node].map(|block| {
                    let class = matches!(alien.head(node), "klass" | "class");
                    let shape = if class {
                        ValueShape::InlineClass
                    } else {
                        ValueShape::InlineFunction
                    };
                    let mut fact = ValueFact::new(shape, text::OTHER);
                    fact.block = Some(block);
                    (fact, ValueInfo::Inline { block, class })
                }),
                "wrap" | "star" => {
                    value_of[node] = item_value(0);
                    None
                }
                "spread" => Some((ValueFact::new(ValueShape::Other, text::OTHER), ValueInfo::Other)),
                _ => None,
            }
        };
        if let Some((fact, info)) = entry {
            value_of[node] = Some(values.len() as u32);
            values.push(fact);
            infos.push(info);
        }
    }
    for (decl, init) in inits {
        match &mut decls[decl].kind {
            DeclKind::Variable { init: slot, .. } | DeclKind::Field { init: slot, .. } => {
                *slot = value_of[init];
            }
            _ => {}
        }
    }

    // Occurrences and the census.
    let mut calls = Vec::new();
    let mut imports = Vec::new();
    let mut call_sites = Vec::new();
    let mut dependencies = Vec::new();
    let mut conditional = Vec::new();
    for node in (1..count).filter(|&node| !skipped[node]) {
        let head = alien.head(node);
        let items = syntax.items(node);
        let range = syntax.node(node).range;
        match head {
            "call" | "new" | "super" => {
                let mut arguments = Vec::new();
                let mut ineligible = Vec::new();
                for (position, &item) in items.iter().enumerate().skip(1) {
                    let position = position as u32 - 1;
                    let reference = alien.head(item) != "star";
                    if !reference {
                        ineligible.push(position);
                    }
                    if let Some(value) = value_of[item] {
                        arguments.push(ArgumentFact {
                            expand: (!reference).then_some(Expansion::Spread),
                            ..ArgumentFact::positional(value, position, reference)
                        });
                    }
                }
                call_sites.push(CallSite {
                    range,
                    new: head == "new",
                    ineligible,
                });
                if let Some(block) = block_of[node] {
                    calls.push(CallFact {
                        block,
                        callee: items.first().and_then(|&item| value_of[item]),
                        edge: if head == "new" {
                            EdgeKind::Constructs
                        } else {
                            EdgeKind::Calls
                        },
                        context: if head == "call" {
                            CallContext::Call
                        } else {
                            CallContext::Construct
                        },
                        arguments,
                    });
                }
            }
            "use" | "import" => {
                dependencies.push(range);
                if let Some(block) = block_of[node].filter(|_| syntax.option(node, "when-indexed").is_some())
                {
                    conditional.push(block);
                }
                if let Some(block) = block_of[node] {
                    let specifier = items
                        .iter()
                        .find(|&&item| syntax.is_literal(item))
                        .map(|&item| unquote(syntax.node(item).text).into());
                    imports.push(ImportFact { block, specifier });
                }
            }
            _ => {}
        }
    }
    let (types, type_names) = typed(&alien, &block_of, &outer, &inner, &skipped, selection.blocks());
    let payload = Payload {
        decls,
        scopes,
        values: infos,
        boundaries: syntax.boundaries.clone(),
        call_sites,
        dependencies,
        types: type_names,
    };
    let graph = GraphFacts::new(selection, values, calls, imports)
        .and_then(|graph| graph.with_conditional(conditional))
        .and_then(|graph| graph.with_types(types))
        .map_err(|error| SourceFailure {
            message: error.to_string().into(),
            offset: None,
        })?;
    let payload = postcard::to_allocvec(&payload).map_err(|error| SourceFailure {
        message: error.to_string().into(),
        offset: None,
    })?;
    Ok(FileFacts::new(graph, payload))
}

/// The range of an option's value inside its `:key=value` token.
fn option_value(syntax: &Syntax<'_>, node: usize, key: &str) -> Option<(ByteRange, String)> {
    let prefix = format!(":{key}=");
    let child = syntax
        .node(node)
        .children
        .iter()
        .copied()
        .find(|&child| syntax.is_option(child) && syntax.node(child).text.starts_with(&prefix))?;
    let token = syntax.node(child);
    let value = &token.text[prefix.len()..];
    let range = ByteRange::new(token.range.end - value.len() as u32, token.range.end).ok()?;
    (!value.is_empty()).then(|| (range, value.to_owned()))
}

/// The typed facts, and each type name's scope and text (the payload's `types`).
fn typed(
    alien: &Alien<'_, '_>,
    block_of: &[Option<u32>],
    outer: &[u32],
    inner: &[u32],
    skipped: &[bool],
    blocks: &[unfold_core::Block],
) -> (TypeFacts, Vec<(u32, String)>) {
    let syntax = alien.syntax;
    let mut signatures = Vec::new();
    let mut parameters = Vec::new();
    let mut results = Vec::new();
    let mut references = Vec::new();
    let mut names = Vec::new();
    for node in (1..syntax.nodes.len()).filter(|&node| !skipped[node]) {
        let head = alien.head(node);
        let Some(block) = block_of[node] else {
            if head == "param"
                && let Some(callable) = syntax.node(node).parent
                && let Some(owner) = block_of[callable]
            {
                let index = syntax
                    .node(callable)
                    .children
                    .iter()
                    .take_while(|&&child| child != node)
                    .filter(|&&child| alien.head(child) == "param")
                    .count() as u32;
                let name = alien.name_text(node).map(str::to_owned);
                let kind = match (name.as_deref(), syntax.option(node, "rest")) {
                    (Some("this"), _) => ParameterKind::This,
                    (_, Some(_)) => ParameterKind::Rest,
                    _ => ParameterKind::Positional,
                };
                let ty = option_value(syntax, node, "type");
                if let Some((range, text)) = &ty {
                    references.push(TypeRefFact::new(*range, text, false));
                    names.push((inner[callable], text.clone()));
                }
                parameters.push(Parameter {
                    block: owner,
                    index,
                    kind,
                    name: name.map(Into::into),
                    optional: false,
                    default: false,
                    ty: ty.map(|(range, text)| WrittenType {
                        text: text.into(),
                        range,
                    }),
                });
            }
            continue;
        };
        let kind = blocks[block as usize].kind;
        if kind.is_function_like() {
            signatures.push(Signature { block, returns: None });
        }
        if kind == BlockKind::Variable
            && let Some((range, text)) = option_value(syntax, node, "type")
        {
            references.push(TypeRefFact::new(range, &text, false));
            names.push((outer[node], text));
        }
        if matches!(kind, BlockKind::Call | BlockKind::New) {
            results.push(result(alien, block_of, node, block));
        }
    }
    let types = TypeFacts::new(blocks, signatures, parameters, Vec::new(), results, references);
    (types, names)
}

/// Where the value of the call at `node` lands, looking through `wrap`.
fn result(alien: &Alien<'_, '_>, block_of: &[Option<u32>], node: usize, call: u32) -> CallResult {
    let syntax = alien.syntax;
    let mut value = node;
    let mut parent = syntax.node(value).parent;
    while let Some(wrapper) = parent.filter(|&wrapper| alien.head(wrapper) == "wrap") {
        value = wrapper;
        parent = syntax.node(value).parent;
    }
    let other = CallResult {
        call,
        binding: ResultBinding::Other,
        awaited: false,
        target: None,
        position: None,
        bound: None,
    };
    let Some(parent) = parent else {
        return other;
    };
    let items = syntax.items(parent);
    let position = items.iter().position(|&item| item == value);
    let (binding, target, position) = match (alien.head(parent), position) {
        ("stmt", Some(0)) => (ResultBinding::Discard, None, None),
        ("ret", Some(0)) => (ResultBinding::Return, None, None),
        ("const" | "let", Some(0)) => (ResultBinding::Variable, block_of[parent], None),
        ("call" | "new" | "super", Some(index)) if index > 0 => {
            (ResultBinding::Argument, block_of[parent], Some(index as u32 - 1))
        }
        _ => return other,
    };
    match target {
        None if binding.eq(&ResultBinding::Variable) || binding.eq(&ResultBinding::Argument) => other,
        _ => CallResult {
            call,
            binding,
            awaited: false,
            target,
            position,
            bound: None,
        },
    }
}
