# Python front end

Read this for Python parsing, imports/source roots, binding, receivers, or typed extraction.
Start at [lib.rs](../../crates/unfold-lang-python/src/lib.rs). Shared outcome precedence and
identity belong to [graph model](../graph-model.md); detector rules to [engine](../engine.md#entrance-detectors).

## Code map

| Concern | Owning module |
| --- | --- |
| Ruff parsing, grammar/compile-time checks | `parse.rs`, `analysis/` |
| Selected syntax, slots, names, facts | `extract.rs`, `mapping.rs`, `ast.rs`, `syntax.rs` |
| Cached binder IR/payload | `bind/`, `payload.rs` |
| Program/import/name/receiver resolution | `link/` |
| Written types | `typed.rs` |
| Occurrence coverage and highlighting | `census.rs`, `highlight.rs`, `tokens.rs` |
| Framework recognition | `detect/`; matching `tests/entrances.rs` and detector fixtures |

Paths are relative to `crates/unfold-lang-python/src/`. Read a module's local contract and the
relevant fixture first.

## Files and parsing

Parse with pinned Ruff under the configured Python grammar (currently 3.12–3.14). Enforce
supported compile-time checks as well as parser syntax. Strict UTF-8 is required; a non-UTF-8
encoding declaration fails even when the bytes otherwise decode. Keep BOM/line endings and
ranges intact; Python name identity uses NFKC while text retains source spelling.

Do not run Python, import packages, or execute annotations/decorators. Failures and depth
limits are explicit; a file may retain visibly last-good facts through the shared update
pipeline. Cached facts contain serializable binder state, not a retained Ruff AST.

Calls remain written `call` blocks. Linking can classify an occurrence edge as `constructs`
when class identity is proven, targeting a permitted concrete `__init__` or the class. Do not
infer constructions from capitalized names.

## Imports and source roots

The deepest configured Python project owns a file. Its roots and grammar affect the per-file
settings hash. Without explicit roots, use the supported src-layout/pyproject defaults.
Module candidates may exist outside indexing; distinguish `not_indexed` from `not_found`.
Multiple roots or module/package collisions can be ambiguous and reported without inventing
a preferred module. Respect relative-import levels, aliases, package identity, and stdlib
classification; never load the real interpreter's import path as proof.

A `from package import name` is not automatically a module load: it may name an ordinary
attribute. Supported structural child-module proofs select import blocks at link time.
Conditional import blocks dropped after resolution must also renumber blocks, keys, slots,
and typed references consistently. Dynamic import calls and arbitrary `__getattr__` exports
are not static module proof.

Implementation entry: [link/modules.rs](../../crates/unfold-lang-python/src/link/modules.rs).

## Binder and runtime uncertainty

Linking builds whole-program scopes, declarations, writes, module/member indexes, class
relationships, and memoized answers from per-file payloads. Preserve Python scope rules for
functions, lambdas, comprehensions, class bodies, globals/nonlocals, parameters, and imports.
A name with several writes or unsupported reflection must not become a unique stable binding
merely because one declaration looks callable.

Attribute/member proof accounts for supported mutations and aliases, including containers
and reflective operations. Unknown writes/reflection can invalidate class members and
framework provenance. Keep reasons explicit through the shared value policy.

C3 linearization governs supported inheritance/member lookup. Class construction and instance
method access bind receivers differently from access through the class. Unknown bases,
decorators, metaclasses, dispatch families, accessors, or mutations can prevent a concrete
answer. Preserve `super` and descriptor/decorator-specific rules; do not unwrap arbitrary
wrappers to reach a convenient body.

Supported registration decorators keep a body only under their detector's proof conditions.
Factories or task/job registries can replace a callable despite an obvious source definition.
For an uncertain registration, list the entrance with an unsupported/wrapper target rather
than claiming its body. See [engine detectors](../engine.md#entrance-detectors).

## Typed records

Record written function parameter/return annotations, variable annotations, direct annotated
class-name assignments, call arguments/results, eligible type names, and static class bases.
Class fields are written properties regardless of whether the class is a dataclass, TypedDict,
Pydantic model, NamedTuple, or ordinary class. No decorator-specific generated fields.

Keep positional-only, positional, keyword-only, variadic positional, and variadic keyword
parameter kinds. Defaults/annotations remain written facts; they do not imply runtime values.
Pair explicit keywords and positional arguments only after target and receiver binding are
proven. Instance calls bind `self`; class access does not automatically do so. Record bound
counts for both calls and passed callables. See [pairing](../graph-model.md#pairing).

Written type-name links exclude lexical PEP 695 parameters, literals and `None`, arguments
of `Literal[...]`, and metadata after the first `Annotated[...]` argument. A string annotation
links only when its entire content is a dotted name; its range is inside the quotes. Do not
parse arbitrary annotation strings as an inferred type language.

Qualified names and imported aliases resolve conservatively in the proper type/value context.
Heritage records use `extends` for ordinary name/member-chain positional bases; generic
arguments get ordinary links. Calls, starred bases, and keyword bases such as `metaclass=`
do not become static heritage.

## Known limits and checks

No interpreter or type checker participates in compilation. Dynamic imports, generated
members, arbitrary decorators, runtime reflection, and uncertain receiver dispatch retain
explicit limits. Written equal argument/parameter ordinals are never binding proof.

Start with `crates/unfold-lang-python/tests/{structure,binder,typed,failures,entrances}.rs`
and relevant `testdata/fixtures/python/` or detector cases. Syntax changes need grammar,
encoding, census, and ranges checks; binder changes need writes, imports, classes, decorators,
reflection, and receiver cases. Detector changes need false-positive and body-proof cases.

Parity/oracle policy lives in [testing](../testing.md#precision-and-recall-gates); preserve
precision and reviewed divergences rather than silently replacing uncertain answers.
