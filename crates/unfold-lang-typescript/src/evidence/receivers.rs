//! Receiver evidence from what the syntax states (the proof of concept's
//! `frontend/native/declared-types.ts`, evidence level 4): written annotations of variables,
//! parameters, properties, and getters; explicit `new`, assertion, and call initializers of a
//! `const` or a variable never assigned again; `as` and `<T>` assertions; declared return types
//! of callees, and through `await` the `Promise` type arguments `async` callees declare; and
//! member chains through class members' declared types. Library receivers (`string`, arrays,
//! `Map`, `NodeJS.Process`, …) come from the generated tables, and so does the ECMAScript type
//! whose methods a receiver has: a primitive or array type, an ECMAScript class type written by
//! name, an ECMAScript global (`JSON`, `Object`, or `globalThis.JSON` by its name) or what `new` of
//! one constructs, and a literal. A library global by name (`console`) is a library receiver.
//! What `new` of a global the project replaced constructs is `dynamic`, and so are its members.
//! A member of a default-library value (`s[0]`, `e.message`, `Math.PI`, `Intl.Collator`) has a
//! type Unfold doesn't read: it gives no receiver, and neither does what `new` of one constructs;
//! a package's member keeps the package's location.
//! A written library type is read nominally, so a project class may still be its value
//! ([`builtins`](super::builtins)); an ECMAScript protocol (`Iterator`, `PromiseLike`,
//! `ReadonlyArray`), `Object`, `object`, or `Function`, which values of any type implement, gives
//! no receiver, and neither does a type query of an ECMAScript global (`typeof Array`), whose
//! static members a project subclass may override. There is no inference, substitution,
//! contextual typing, or flow narrowing: an unknown constituent, disagreeing declarations, or a
//! narrowed receiver give no receiver.

use smallvec::SmallVec;
use unfold_engine::evidence::ExternalLocation;
use unfold_engine::{DeclRef, FileId};

use super::{Receiver, Target, Walk};
use crate::link::TypeScriptLinked;
use crate::payload::binding::{
    BuiltinType, Decl, DeclFlags, DeclKind, Expr, Lookup, Meanings, TypeRef, VarKind,
};
use crate::tables;

/// A member access whose receiver is being evaluated.
struct Access<'e> {
    /// The receiver expression.
    object: u32,
    /// The member name (for an element access, a key written directly as a literal).
    name: Option<&'e str>,
    /// A private name's members.
    private: Option<&'e [u32]>,
    /// The access reads an array element.
    element: bool,
}

/// The library types whose values are arrays.
const ARRAYS: [&str; 2] = ["Array", "globalThis.Array"];

/// Where a library type is declared: Node's own namespace comes from `@types/node` (so a
/// receiver of that type is labeled `node`, never `global`: C-6); everything else is the
/// default library.
pub(crate) fn library_location(qualified: &str) -> ExternalLocation {
    let name = qualified.strip_prefix("globalThis.").unwrap_or(qualified);
    if name.starts_with("NodeJS.") {
        ExternalLocation::Package("@types/node".into())
    } else {
        ExternalLocation::Library
    }
}

impl TypeScriptLinked<'_> {
    /// The declared-type receiver of expression `index` (the proof of concept's
    /// `declaredTypes.receiver`).
    pub(crate) fn receiver(&self, walk: &mut Walk, file: FileId, index: u32) -> Receiver {
        if let Some(done) = walk.receiver_memo.get(&(file, index)) {
            return done.clone();
        }
        if !walk.receivers.insert((file, index)) || !walk.enter() {
            return Receiver::default();
        }
        let receiver = self.receiver_uncached(walk, file, index);
        walk.leave();
        walk.receivers.remove(&(file, index));
        walk.receiver_memo.insert((file, index), receiver.clone());
        receiver
    }

    fn receiver_uncached(&self, walk: &mut Walk, file: FileId, index: u32) -> Receiver {
        let Some(expr) = self.expr(file, index) else {
            return Receiver::default();
        };
        match expr {
            Expr::Paren(inner) | Expr::NonNull(inner) | Expr::Satisfies(inner) => {
                return self.receiver(walk, file, *inner);
            }
            Expr::Assert { ty, .. } => {
                return ty.map_or_else(Receiver::default, |ty| self.type_at(walk, file, ty));
            }
            _ => {}
        }
        if self.is_narrowed(file, index) {
            return Receiver::default();
        }
        match expr {
            Expr::This(context) => self.this_receiver(file, *context),
            Expr::Super(context) => self.super_receiver(walk, file, *context),
            Expr::Name { name, lookup } => {
                if !self.has_symbol(file, name, *lookup) {
                    if *lookup == Lookup::Dynamic || !tables::is_known_global(name) {
                        return Receiver::default();
                    }
                    return library_global(name);
                }
                let target = self.symbol_lookup(walk, file, name, *lookup, Meanings::VALUE);
                self.declared_receiver(walk, &target, true)
            }
            Expr::New { callee: Some(callee) } => self.constructed(walk, file, *callee),
            Expr::Call { callee: Some(callee) } | Expr::Tagged { tag: callee } => {
                let signature = self.unwrap_receiver(file, *callee);
                if matches!(self.expr(file, signature), Some(Expr::Assert { .. })) {
                    return Receiver::default();
                }
                let target = self.target(walk, file, *callee);
                self.return_type(walk, &target)
            }
            Expr::Await(operand) => self.awaited(walk, file, *operand),
            Expr::Literal(builtin) => Receiver {
                builtin: Some(builtin.interface()),
                ..Receiver::default()
            },
            Expr::Member {
                object,
                name,
                private,
            } => {
                let access = Access {
                    object: *object,
                    name: Some(name),
                    private: private.as_deref(),
                    element: false,
                };
                self.member_receiver(walk, file, index, &access)
            }
            Expr::Element {
                object,
                key,
                direct,
                index: element,
            } => {
                let access = Access {
                    object: *object,
                    name: key.as_deref().filter(|_| *direct),
                    private: None,
                    element: *element,
                };
                self.member_receiver(walk, file, index, &access)
            }
            _ => Receiver::default(),
        }
    }

    /// The receiver of a name or member target: its external or excluded boundary, its
    /// declarations' written types (when any of them is annotated), the class it is (a static
    /// receiver, when `classes` allows), or its declarations' declared types.
    fn declared_receiver(&self, walk: &mut Walk, target: &Target, classes: bool) -> Receiver {
        if target.ambiguous {
            return Receiver::default();
        }
        if !target.excluded.is_empty() {
            return Receiver::excluded(target.excluded.iter().cloned());
        }
        if !target.external.is_empty() {
            return Receiver::external(target.external.iter().cloned(), false);
        }
        let annotated = target.declarations.iter().any(|&declaration| {
            self.decl(declaration).is_some_and(|decl| {
                decl.ty.is_some()
                    && matches!(
                        decl.kind,
                        DeclKind::Variable | DeclKind::Parameter | DeclKind::Property
                    )
            })
        });
        if !annotated
            && classes
            && let Some(class) = self.class_for_target(target)
        {
            return Receiver::class(class, true);
        }
        let receivers: SmallVec<[Receiver; 2]> = target
            .declarations
            .iter()
            .map(|&declaration| self.declaration_type(walk, declaration))
            .collect();
        consensus(receivers)
    }

    /// The receiver `new callee` constructs. `new` of an ECMAScript global constructs the
    /// interface of the same name (`new Map()` a `Map`), unless the project replaced the global.
    fn constructed(&self, walk: &mut Walk, file: FileId, callee: u32) -> Receiver {
        if self
            .global_value(file, callee)
            .is_some_and(|global| self.global_replaced(global))
        {
            return Receiver::dynamic();
        }
        let mut written = callee;
        while let Some(
            Expr::Paren(inner) | Expr::NonNull(inner) | Expr::Satisfies(inner) | Expr::Instantiation(inner),
        ) = self.expr(file, written)
        {
            written = *inner;
        }
        if !matches!(self.expr(file, written), Some(Expr::Assert { .. }))
            && let Some(global) = self.global_value(file, written)
        {
            return Receiver {
                array: global == "Array",
                builtin: tables::ecmascript_class(global),
                ..Receiver::external([ExternalLocation::Library], false)
            };
        }
        let constructor = self.receiver(walk, file, callee);
        if constructor.excluded {
            return constructor;
        }
        if constructor.class.is_some() || constructor.external {
            return if constructor.is_static || constructor.external {
                Receiver {
                    is_static: false,
                    ..constructor
                }
            } else {
                Receiver::default()
            };
        }
        if matches!(self.expr(file, written), Some(Expr::Assert { .. })) {
            return Receiver::default();
        }
        let target = self.target(walk, file, callee);
        let annotated = target.declarations.iter().any(|&declaration| {
            self.decl(declaration).is_some_and(|decl| {
                decl.ty.is_some()
                    && matches!(
                        decl.kind,
                        DeclKind::Variable | DeclKind::Property | DeclKind::Parameter
                    )
            })
        });
        // What `new` of a member of a default-library value constructs (`new Intl.Collator()`)
        // isn't read.
        if annotated || is_default_library(&target.external) {
            return Receiver::default();
        }
        self.target_receiver(&target, false)
    }

    /// The receiver of a member access: its own target's class or declared types, else the
    /// declared types of the receiver class's member.
    fn member_receiver(&self, walk: &mut Walk, file: FileId, index: u32, access: &Access<'_>) -> Receiver {
        if let Some(global) = self.global_value(file, index) {
            return library_global(global);
        }
        let object = access.object;
        if access.element && self.receiver(walk, file, object).array {
            return Receiver::default();
        }
        let direct = self.target(walk, file, index);
        if direct.ambiguous {
            return Receiver::default();
        }
        if !direct.excluded.is_empty() {
            return Receiver::excluded(direct.excluded.iter().cloned());
        }
        if !direct.external.is_empty() {
            return outside_member(direct.external.iter().cloned());
        }
        if let Some(class) = self.class_for_target(&direct) {
            return Receiver::class(class, true);
        }
        if !direct.declarations.is_empty() {
            let receivers: SmallVec<[Receiver; 2]> = direct
                .declarations
                .iter()
                .map(|&declaration| self.declaration_type(walk, declaration))
                .collect();
            return consensus(receivers);
        }
        let base = self.receiver(walk, file, object);
        if base.excluded || base.dynamic {
            return base;
        }
        if base.external {
            return outside_member(base.locations);
        }
        let Some(class) = base.class.filter(|_| !base.unbound_this) else {
            return Receiver::default();
        };
        let Some(name) = access.name else {
            return Receiver::default();
        };
        let target = match access.private {
            Some(private) => Target::of(private.iter().map(|&index| DeclRef { file, index })),
            None => self.lookup_member(walk, class, name, base.is_static),
        };
        if target.ambiguous {
            return Receiver::default();
        }
        if !target.excluded.is_empty() {
            return Receiver::excluded(target.excluded.iter().cloned());
        }
        if !target.external.is_empty() {
            return outside_member(target.external.iter().cloned());
        }
        let receivers: SmallVec<[Receiver; 2]> = target
            .declarations
            .iter()
            .map(|&declaration| self.declaration_type(walk, declaration))
            .collect();
        consensus(receivers)
    }

    /// A declaration's declared receiver: its written type, or an explicit `new`, assertion,
    /// call, or `await` initializer of a variable that is a `const` or is never assigned again,
    /// or of a property never written after its declaration (a parameter's default is not what
    /// an argument passes).
    pub(crate) fn declaration_type(&self, walk: &mut Walk, declaration: DeclRef) -> Receiver {
        let Some(decl) = self.decl(declaration) else {
            return Receiver::default();
        };
        if !matches!(
            decl.kind,
            DeclKind::Variable | DeclKind::Getter | DeclKind::Parameter | DeclKind::Property
        ) {
            return Receiver::default();
        }
        if let Some(ty) = decl.ty {
            return self.type_at(walk, declaration.file, ty);
        }
        let held = match decl.kind {
            DeclKind::Variable => {
                matches!(decl.var_kind, Some(VarKind::Const | VarKind::Using))
                    || !self.tables.reassigned.contains(&declaration)
            }
            DeclKind::Parameter => false,
            DeclKind::Property => !self.tables.written_fields.contains(&declaration),
            _ => true,
        };
        let Some(mut init) = decl.initializer.filter(|_| held) else {
            return Receiver::default();
        };
        while let Some(Expr::Paren(inner) | Expr::NonNull(inner) | Expr::Satisfies(inner)) =
            self.expr(declaration.file, init)
        {
            init = *inner;
        }
        match self.expr(declaration.file, init) {
            Some(Expr::New { .. } | Expr::Assert { .. } | Expr::Call { .. } | Expr::Await(_)) => {
                self.receiver(walk, declaration.file, init)
            }
            _ => Receiver::default(),
        }
    }

    /// The receiver a written type describes.
    pub(crate) fn type_at(&self, walk: &mut Walk, file: FileId, ty: u32) -> Receiver {
        if !walk.types.insert((file, ty)) || !walk.enter() {
            return Receiver::default();
        }
        let receiver = match self
            .binding(file)
            .and_then(|binding| binding.types.get(ty as usize))
        {
            Some(TypeRef::Array) => Receiver {
                array: true,
                builtin: tables::ecmascript_type("Array"),
                nominal: true,
                ..Receiver::external([ExternalLocation::Library], false)
            },
            Some(TypeRef::Primitive(BuiltinType::Object)) => Receiver::default(),
            Some(TypeRef::Primitive(builtin)) => Receiver {
                builtin: tables::ecmascript_type(builtin.interface()),
                ..Receiver::external([ExternalLocation::Library], false)
            },
            Some(TypeRef::Names {
                names,
                value,
                lookup,
                shadowed,
            }) => self.type_names(walk, file, names, *value, *lookup, *shadowed),
            _ => Receiver::default(),
        };
        walk.leave();
        walk.types.remove(&(file, ty));
        receiver
    }

    /// The receiver a type reference (or, with `value`, a type query) names.
    fn type_names(
        &self,
        walk: &mut Walk,
        file: FileId,
        names: &[Box<str>],
        value: bool,
        lookup: Lookup,
        shadowed: bool,
    ) -> Receiver {
        let Some((first, rest)) = names.split_first() else {
            return Receiver::default();
        };
        let meaning = if value { Meanings::VALUE } else { Meanings::TYPE };
        let symbol = match lookup {
            Lookup::Symbol(symbol) => Some(self.resolve_symbol(walk, file, symbol, meaning)),
            Lookup::Unresolved | Lookup::Namespaced(_) => self.unresolved(walk, file, first, lookup, meaning),
            Lookup::Dynamic => None,
        };
        let Some(mut target) = symbol else {
            let shadow = shadowed || (lookup != Lookup::Dynamic && self.has_global(file, first));
            let qualified = names.join(".");
            if shadow || !tables::is_library_type(&qualified) {
                return Receiver::default();
            }
            if value {
                // A project class that extends an ECMAScript global may override its static
                // members, and other objects may match its type.
                if tables::is_ecmascript_global(qualified.strip_prefix("globalThis.").unwrap_or(&qualified)) {
                    return Receiver::default();
                }
                return Receiver::external([library_location(&qualified)], true);
            }
            // An ECMAScript type is a receiver only as a class type: values of any type implement
            // a protocol, every value is an `Object`, and every function and class a `Function`.
            let builtin = match tables::ecmascript_type_name(&qualified) {
                Some(_) => match tables::ecmascript_class(&qualified)
                    .filter(|class| !matches!(*class, "Object" | "Function"))
                {
                    Some(class) => Some(class),
                    None => return Receiver::default(),
                },
                None => None,
            };
            return Receiver {
                array: ARRAYS.contains(&qualified.as_str()),
                builtin,
                nominal: true,
                ..Receiver::external([library_location(&qualified)], false)
            };
        };
        for name in rest {
            target = self.member(walk, &target, name, meaning);
        }
        let direct = self.target_receiver(&target, value);
        if direct.class.is_some() || direct.external || target.ambiguous || !target.excluded.is_empty() {
            return direct;
        }
        if target.declarations.is_empty()
            || target.declarations.iter().any(|&declaration| {
                self.decl(declaration)
                    .is_none_or(|decl| decl.kind != DeclKind::TypeAlias)
            })
        {
            return Receiver::default();
        }
        let receivers: SmallVec<[Receiver; 2]> = target
            .declarations
            .iter()
            .map(
                |&declaration| match self.decl(declaration).and_then(|decl| decl.ty) {
                    Some(ty) => self.type_at(walk, declaration.file, ty),
                    None => Receiver::default(),
                },
            )
            .collect();
        consensus(receivers)
    }

    /// The receiver a target is as a value of its own: an excluded boundary, its class, or an
    /// external value.
    pub(crate) fn target_receiver(&self, target: &Target, is_static: bool) -> Receiver {
        if target.ambiguous {
            return Receiver::default();
        }
        if !target.excluded.is_empty() {
            return Receiver::excluded(target.excluded.iter().cloned());
        }
        if target.external.is_empty()
            && let Some(class) = self.class_for_target(target)
        {
            return Receiver::class(class, is_static);
        }
        if !target.external.is_empty() && target.declarations.is_empty() {
            return Receiver::external(target.external.iter().cloned(), is_static);
        }
        Receiver::default()
    }

    /// The receiver a call to a target returns: the declared return types of its callables
    /// (overload signatures included), when all of them agree and one has a body.
    pub(crate) fn return_type(&self, walk: &mut Walk, target: &Target) -> Receiver {
        self.callables_type(walk, target, |this, walk, callable, decl| match decl.returns {
            Some(ty) => this.type_at(walk, callable.file, ty),
            None => Receiver::default(),
        })
    }

    /// The receiver `await` of `operand` gives: for a call, what the `async` callables it
    /// reaches declare their `Promise` resolves to, when all of them agree and one has a body.
    /// Anything else gives none.
    fn awaited(&self, walk: &mut Walk, file: FileId, operand: u32) -> Receiver {
        let operand = self.unwrap_receiver(file, operand);
        let Some(Expr::Call { callee: Some(callee) }) = self.expr(file, operand) else {
            return Receiver::default();
        };
        let signature = self.unwrap_receiver(file, *callee);
        if matches!(self.expr(file, signature), Some(Expr::Assert { .. })) {
            return Receiver::default();
        }
        let target = self.target(walk, file, *callee);
        self.callables_type(walk, &target, |this, walk, callable, decl| {
            let (Some(returns), Some(awaits)) = (decl.returns, decl.awaits) else {
                return Receiver::default();
            };
            // An `async` callable's `Promise` is the library's.
            let promise = this.type_at(walk, callable.file, returns);
            if promise.class.is_some() || promise.locations.as_slice() != [ExternalLocation::Library] {
                return Receiver::default();
            }
            this.type_at(walk, callable.file, awaits)
        })
    }

    /// The receiver `declared` reads from each callable of a target (overload signatures
    /// included), when all of them agree and one has a body.
    fn callables_type(
        &self,
        walk: &mut Walk,
        target: &Target,
        declared: impl Fn(&Self, &mut Walk, DeclRef, &Decl) -> Receiver,
    ) -> Receiver {
        if target.ambiguous || !target.excluded.is_empty() || !target.external.is_empty() {
            return Receiver::default();
        }
        let callables: SmallVec<[Option<DeclRef>; 2]> = target
            .declarations
            .iter()
            .filter(|&&declaration| {
                self.decl(declaration)
                    .is_some_and(|decl| !matches!(decl.kind, DeclKind::Namespace | DeclKind::Interface))
            })
            .map(|&declaration| self.callable(declaration))
            .collect();
        if callables.is_empty() || callables.iter().any(Option::is_none) {
            return Receiver::default();
        }
        let has_body = callables.iter().flatten().any(|&callable| {
            self.decl(callable)
                .is_some_and(|decl| decl.flags.has(DeclFlags::HAS_BODY))
        });
        if !has_body {
            return Receiver::default();
        }
        let receivers: SmallVec<[Receiver; 2]> = callables
            .into_iter()
            .flatten()
            .map(|callable| match self.decl(callable) {
                Some(decl) => declared(self, walk, callable, decl),
                None => Receiver::default(),
            })
            .collect();
        consensus(receivers)
    }

    /// The callable a declaration is: a function, method, or arrow; or the function or arrow
    /// an unannotated property or `const` is initialized with.
    pub(crate) fn callable(&self, declaration: DeclRef) -> Option<DeclRef> {
        let decl = self.decl(declaration)?;
        match decl.kind {
            DeclKind::Function | DeclKind::FunctionExpression | DeclKind::Arrow | DeclKind::Method => {
                Some(declaration)
            }
            DeclKind::Property | DeclKind::Variable
                if decl.kind == DeclKind::Property || decl.var_kind == Some(VarKind::Const) =>
            {
                if decl.ty.is_some() {
                    return None;
                }
                let init = self.unwrap_expr(declaration.file, decl.initializer?);
                match self.expr(declaration.file, init)? {
                    Expr::Function(function) => Some(DeclRef {
                        file: declaration.file,
                        index: *function,
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// The value of the library global `name` (`console`, or `globalThis.JSON` by its name): outside
/// the project, with the methods of its type when it is ECMAScript's (`JSON`, `Object`).
fn library_global(name: &str) -> Receiver {
    Receiver {
        builtin: tables::ecmascript_value_type(name),
        ..Receiver::external([ExternalLocation::Library], false)
    }
}

/// Whether any of `locations` is the default library (labeled `global` or `ecmascript`). Its
/// values are typed only by name, a written type, or a literal, never through a member.
pub(super) fn is_default_library(locations: &[ExternalLocation]) -> bool {
    locations
        .iter()
        .any(|location| matches!(location, ExternalLocation::Library | ExternalLocation::Builtin))
}

/// The receiver a member of an outside value at `locations` is: a package's or file's member
/// keeps its location (`fs.promises`), but a member of a default-library value (`s[0]`,
/// `e.message`, `Math.PI`, `Function.prototype`, `Intl.Collator`) has a type Unfold doesn't read.
fn outside_member(locations: impl IntoIterator<Item = ExternalLocation>) -> Receiver {
    let locations: SmallVec<[ExternalLocation; 1]> = locations.into_iter().collect();
    if is_default_library(&locations) {
        return Receiver::default();
    }
    Receiver::external(locations, false)
}

/// The one receiver every declaration agrees on; anything unknown or disagreeing gives none.
fn consensus(receivers: SmallVec<[Receiver; 2]>) -> Receiver {
    let Some(first) = receivers.first() else {
        return Receiver::default();
    };
    let known = first.class.is_some() || first.external || first.excluded || first.dynamic;
    let agree = receivers.iter().all(|item| {
        item.class == first.class
            && item.external == first.external
            && item.excluded == first.excluded
            && item.array == first.array
            && item.builtin == first.builtin
            && item.nominal == first.nominal
            && item.is_static == first.is_static
            && item.dynamic == first.dynamic
            && !item.unbound_this
    });
    if !known || !agree {
        return Receiver::default();
    }
    if !(first.external || first.excluded) {
        return first.clone();
    }
    let mut merged = first.clone();
    merged.locations.clear();
    for item in &receivers {
        for location in &item.locations {
            if !merged.locations.contains(location) {
                merged.locations.push(location.clone());
            }
        }
    }
    merged
}
