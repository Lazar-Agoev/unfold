//! The evidence the shared policy reads for a value (the proof of concept's
//! `expressionEvidence` and `annotationEvidence`): candidates from the value's target, the
//! receiver class, static and `super` access, an unbound `this`, ambiguity from members that
//! may override through an unknown base, and the access root's import binding. A written type
//! name's evidence ([`TypeScriptLinked::type_facts`]) is looked up in the type meaning, with
//! undeclared library types and global type aliases external.

use std::borrow::Cow;

use unfold_engine::evidence::{Candidate, Evidence, ImportBinding, ImportTarget, Origin};
use unfold_engine::{DeclRef, FileId, TypeRef as TypeName};

use super::{Target, Walk};
use crate::link::{TypeScriptLinked, import_kind};
use crate::modules::Resolved;
use crate::payload::ImportSource;
use crate::payload::binding::{AliasRef, DeclFlags, DeclKind, Expr, Lookup, Meanings, TypeRef};
use crate::tables;
use unfold_engine::evidence::ExternalLocation;

impl TypeScriptLinked<'_> {
    /// The evidence for value `value` of `file` (an index into the neutral value table).
    pub(crate) fn value_facts(&self, file: FileId, value: u32) -> Evidence {
        let Some(&expr) = self
            .binding(file)
            .and_then(|binding| binding.value_exprs.get(value as usize))
        else {
            return Evidence::default();
        };
        self.expression_evidence(file, expr)
    }

    /// The evidence for expression `index` of `file`.
    fn expression_evidence(&self, file: FileId, index: u32) -> Evidence {
        let mut walk = Walk::default();
        let expression = self.unwrap_expr(file, index);
        let target = self.target(&mut walk, file, expression);
        let (access, name) = match self.expr(file, expression) {
            Some(Expr::Member { object, name, .. }) => (Some(*object), Some(&**name)),
            Some(Expr::Element { object, key, .. }) => (Some(*object), key.as_deref()),
            _ => (None, None),
        };
        let receiver_expr = access.map(|object| self.unwrap_expr(file, object));
        let receiver = access.map(|object| self.class_receiver(&mut walk, file, object));
        let mut root = receiver_expr.unwrap_or(expression);
        while let Some(Expr::Member { object, .. } | Expr::Element { object, .. }) = self.expr(file, root) {
            root = self.unwrap_expr(file, *object);
        }
        let root_name = match self.expr(file, root) {
            Some(Expr::Name { name, lookup }) => Some((&**name, *lookup)),
            _ => None,
        };
        let root_symbol = root_name.filter(|&(name, lookup)| self.has_symbol(file, name, lookup));
        let unbound_this = match self.expr(file, root) {
            Some(Expr::This(context)) => self.this_receiver(file, *context).unbound_this,
            _ => false,
        };
        let typed_receiver = access.map(|object| self.unwrap_receiver(file, object));
        let receiver_class = match typed_receiver.and_then(|typed| self.expr(file, typed)) {
            Some(Expr::This(context)) => self.containing_class(file, *context),
            _ => receiver.as_ref().and_then(|receiver| receiver.class),
        };
        let candidates = self.candidates(&target);
        let static_access = target.declarations.iter().any(|&declaration| {
            self.decl(declaration)
                .is_some_and(|decl| decl.flags.has(DeclFlags::STATIC))
        });
        let super_access = matches!(self.expr(file, expression), Some(Expr::Super(_)))
            || receiver_expr
                .is_some_and(|receiver| matches!(self.expr(file, receiver), Some(Expr::Super(_))));
        let overridable = target.declarations.iter().any(|&declaration| {
            self.decl(declaration)
                .is_some_and(|decl| matches!(decl.kind, DeclKind::Method | DeclKind::Property))
        });
        let unknown_override = !static_access
            && !super_access
            && overridable
            && name.is_some_and(|name| self.may_have_unknown_override(receiver_class, name));
        let import_binding = root_name.and_then(|(_, lookup)| self.import_binding(file, lookup));
        Evidence {
            candidates: candidates.into_iter().collect(),
            refusal: None,
            ambiguous: target.ambiguous || unknown_override,
            overridden: false,
            receiver_class,
            super_access,
            static_access,
            unbound_this,
            import_binding,
            root_is_indexed: Some(root_symbol.is_some()),
        }
    }

    /// A target's candidates: external locations, excluded boundaries, then declarations
    /// (namespaces, interfaces, and type aliases only augment).
    pub(crate) fn candidates(&self, target: &Target) -> Vec<Candidate> {
        let mut candidates =
            Vec::with_capacity(target.external.len() + target.excluded.len() + target.declarations.len());
        for location in &target.external {
            candidates.push(Candidate::external(location.clone()));
        }
        for location in &target.excluded {
            candidates.push(Candidate {
                origin: Origin::Excluded,
                declaration: None,
                augmentation: false,
                location: Some(location.clone()),
            });
        }
        for &declaration in &target.declarations {
            let mut candidate = Candidate::indexed(declaration);
            candidate.augmentation = self
                .decl(declaration)
                .is_some_and(|decl| decl.kind.is_augmentation());
            candidates.push(candidate);
        }
        candidates
    }

    /// A target's candidates in the type context: as [`TypeScriptLinked::candidates`], but only
    /// namespaces augment (an interface or type alias is a type declaration of its own).
    fn type_candidates(&self, target: &Target) -> Vec<Candidate> {
        let mut candidates = self.candidates(target);
        for candidate in &mut candidates {
            if let Some(declaration) = candidate.declaration {
                candidate.augmentation = self
                    .decl(declaration)
                    .is_some_and(|decl| decl.kind == DeclKind::Namespace);
            }
        }
        candidates
    }

    /// The evidence for a written type name (a type reference fact): its first name's lookup in
    /// the type meaning (through type-only imports, namespace imports, and script globals), then
    /// each further name as a member in the type meaning. A name with no type declaration in
    /// scope that is a library class, interface, or global type alias is external with an
    /// unindexed root: one of ECMAScript's own types is a builtin (labeled `ecmascript`), and
    /// any other is at the library (so it labels `global`).
    pub(crate) fn type_facts(&self, reference: TypeName) -> Evidence {
        let file = reference.file;
        let Some(binding) = self.binding(file) else {
            return Evidence::default();
        };
        let ty = binding
            .type_links
            .get(reference.index as usize)
            .and_then(|&ty| binding.types.get(ty as usize));
        let Some(TypeRef::Names {
            names,
            value: false,
            lookup,
            ..
        }) = ty
        else {
            return Evidence::default();
        };
        let Some((first, rest)) = names.split_first() else {
            return Evidence::default();
        };
        let mut walk = Walk::default();
        let found = match lookup {
            Lookup::Symbol(symbol) => Some(self.resolve_symbol(&mut walk, file, *symbol, Meanings::TYPE)),
            Lookup::Unresolved | Lookup::Namespaced(_) => {
                self.unresolved(&mut walk, file, first, *lookup, Meanings::TYPE)
            }
            Lookup::Dynamic => None,
        };
        let Some(mut target) = found else {
            // The lookup was in the type meaning, so no value (local or script global) hides a
            // library type here.
            let qualified = names.join(".");
            let library = *lookup != Lookup::Dynamic
                && (tables::is_library_type(&qualified) || tables::is_library_type_alias(&qualified));
            let candidates = if !library {
                Vec::new()
            } else if tables::is_ecmascript_type(&qualified) {
                vec![Candidate::external(ExternalLocation::Builtin)]
            } else {
                vec![Candidate::external(super::receivers::library_location(
                    &qualified,
                ))]
            };
            return Evidence {
                candidates: candidates.into_iter().collect(),
                root_is_indexed: Some(false),
                ..Evidence::default()
            };
        };
        for name in rest {
            target = self.member(&mut walk, &target, name, Meanings::TYPE);
        }
        Evidence {
            candidates: self.type_candidates(&target).into_iter().collect(),
            ambiguous: target.ambiguous,
            import_binding: self.import_binding(file, *lookup),
            root_is_indexed: Some(true),
            ..Evidence::default()
        }
    }

    /// The import binding of an access root's name: the module import it is an alias of.
    fn import_binding(&self, file: FileId, lookup: Lookup) -> Option<ImportBinding> {
        let Lookup::Symbol(symbol) = lookup else {
            return None;
        };
        let symbol = self.binding(file)?.symbols.get(symbol as usize)?;
        let AliasRef::Module(index) = symbol.alias? else {
            return None;
        };
        self.module_binding(file, index)
    }

    /// The evidence of module import binding `index` of `file`.
    pub(crate) fn module_binding(&self, file: FileId, index: u32) -> Option<ImportBinding> {
        let binding = self.module_facts(file)?.imports.get(index as usize)?;
        let ImportSource::Module { specifier, require } = &binding.source else {
            return None;
        };
        let resolved = match self.import_resolution(file, index) {
            Some(resolved) => Cow::Borrowed(resolved),
            None => Cow::Owned(self.resolve(file, specifier, import_kind(*require))),
        };
        let (target, location) = match &*resolved {
            Resolved::Indexed(_) => (ImportTarget::Indexed, None),
            Resolved::Failed(_) | Resolved::NotIndexed => (ImportTarget::NotIndexed, None),
            Resolved::Unindexed { path, .. } => (
                ImportTarget::External,
                Some(ExternalLocation::File(path.as_str().into())),
            ),
            Resolved::Package(name) => (
                ImportTarget::External,
                Some(ExternalLocation::Package(name.clone())),
            ),
            Resolved::NotFound => (ImportTarget::NotFound, None),
        };
        Some(ImportBinding {
            specifier: specifier.clone(),
            imported: binding.imported.clone(),
            target,
            location,
        })
    }

    /// Written type-reference evidence on a parameter, variable, or property.
    pub(crate) fn annotation_facts(&self, declaration: DeclRef) -> Option<Evidence> {
        let decl = self.decl(declaration)?;
        if !matches!(
            decl.kind,
            DeclKind::Parameter | DeclKind::Variable | DeclKind::Property
        ) {
            return None;
        }
        let file = declaration.file;
        let ty = self.binding(file)?.types.get(decl.ty? as usize)?;
        let TypeRef::Names {
            names,
            value: false,
            lookup,
            ..
        } = ty
        else {
            return None;
        };
        let (first, rest) = names.split_first()?;
        let mut walk = Walk::default();
        let mut target = self.symbol_lookup(&mut walk, file, first, *lookup, Meanings::TYPE);
        for name in rest {
            target = self.member(&mut walk, &target, name, Meanings::TYPE);
        }
        let symbol = match lookup {
            Lookup::Symbol(_) => true,
            Lookup::Unresolved | Lookup::Namespaced(_) => self
                .unresolved(&mut walk, file, first, *lookup, Meanings::TYPE)
                .is_some(),
            Lookup::Dynamic => false,
        };
        Some(Evidence {
            candidates: self.candidates(&target).into_iter().collect(),
            ambiguous: target.ambiguous,
            import_binding: self.import_binding(file, *lookup),
            root_is_indexed: Some(symbol),
            ..Evidence::default()
        })
    }
}
