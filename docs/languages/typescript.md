# TypeScript front end

Read this for TypeScript parsing, project/module resolution, binding, or typed extraction.
Start at [lib.rs](../../crates/unfold-lang-typescript/src/lib.rs). Shared outcome precedence
and identity belong to [graph model](../graph-model.md); detector rules to [engine](../engine.md#entrance-detectors).

## Code map

| Concern | Owning module |
| --- | --- |
| Parse/extract and cached payload | `extract.rs`, `payload/`, `scan.rs` |
| Selected structure/roles/names | `mapping.rs`, `build/`, `names.rs`, `syntax.rs` |
| Project config and paths | `config/`, `paths.rs` |
| Modules, imports, exports, emitted paths | `modules/`, `imports.rs`, `exports/`, `emit/` |
| Scope/name binding | `bind/` |
| Link-time evidence, receivers, declarations | `evidence/`, `link.rs`, `values.rs` |
| Written types | `typed.rs` |
| Runtime library identity | `tables/`; regenerate with `cargo xtask gen-lib-tables` |
| Detectors | `detect/`; matching `tests/entrances.rs` and detector fixtures |

Paths are relative to `crates/unfold-lang-typescript/src/`. Read the relevant module comment
and fixture before expanding to other stages.

## Files and parsing

Use the pinned oxc parser. Source kind derives from the extension/project rules, including
TS, TSX, JS, JSX, module, and CommonJS variants. Preserve token-exact ranges and source text;
never pretty-print ASTs to manufacture code ranges. Ambient/declaration syntax contributes
context where appropriate, without supplying executable body proof.

Parse failures, invalid UTF-8, oversized input, and depth safeguards are explicit failures.
Do not commit parser-recovered fragments as a trustworthy complete graph. Extraction builds
per-file serializable facts; linking may read the whole program but cannot depend on retained
parser arenas. The independent census checks selected occurrences.

Selection is deliberate rather than every AST node becoming a block. Preserve written
statement/occurrence order, role assignment, slots, and structural keys when adding syntax.
Type-only syntax does not become a runtime call/import. Calls in supported computed names,
initializers, and enum members still need occurrence coverage.

## Project configuration

Read applicable `tsconfig` chains and package inputs without running tooling. Configurations,
path aliases, module settings, and file inclusion affect lookup and extraction hashes.
Context files and declarations may inform names but remain distinct from indexed source.

Resolve relative, package, alias, and Node builtin imports through the resolver's in-memory
view of discovery. A repository file outside indexing is `not_indexed`, never external merely
because scope excluded it. An import can be external, missing, or ambiguous while the importing
file still supplies its own structure.

Static imports, exports/re-exports, supported CommonJS shapes, and literal dynamic imports
retain written module identity. Follow export aliases and stars with ambiguity/cycle handling;
do not guess one declaration from conflicting exports. The emit map relates declared output
paths to source before generated output exists. Colliding emit candidates are ambiguous.

Implementation: [config/mod.rs](../../crates/unfold-lang-typescript/src/config/mod.rs),
[modules/mod.rs](../../crates/unfold-lang-typescript/src/modules/mod.rs),
[exports/mod.rs](../../crates/unfold-lang-typescript/src/exports/mod.rs).

## Binding and evidence

The custom binder supplies lexical meanings, scopes, declaration ownership, and `this`
contexts needed by Unfold. A general-purpose name resolver alone does not establish runtime
call targets. Preserve target-dependent parameter scopes, merged declarations/namespaces,
hoisting, catch/class scopes, type/value meanings, and overload compatibility.

Evidence can come from direct lexical bindings, supported constant values/member chains,
written annotations, imports, factories, and receiver contexts. Stable aliases can preserve
identity; mutable or ambiguous bindings cannot become proven targets simply because one
assignment appears nearby. Destructuring, computed members, accessors, and unknown receivers
need their specific conservative handling.

`this` follows member/arrow ownership; an unbound non-arrow `this` is explicit uncertainty.
`super`, static, private, and instance accesses have different dispatch rules. The whole-program
class/override census checks known descendants, unknown bases, and computed member names;
a method body alone does not prove which implementation a runtime receiver dispatches to.

The write census and provenance must notice alias/member writes and supported reflective
mutations. Matching names such as `express`, `Router`, or `Worker` are insufficient framework
proof. Ambient definitions and imported external declarations do not turn into local bodies.

A compatible overload signature beside its implementation is one concrete target. Function,
method, constructor, constant callable initializer, class, handler, and type contexts have
separate permitted targets. Use the shared resolution policy to select reasons consistently.

### Literal awaited dynamic imports

A plain `const` destructuring of a directly awaited literal import can bind the module's
export, e.g. `const { report } = await import("./report.ts")`. Defaults, rest, nested/computed
patterns, mutable bindings, non-awaited imports, and dynamic specifiers do not receive this
special proof. Its fixture and justified oracle disagreement are tracked under
`testdata/fixtures/typescript-evidence/c8-dynamic-import` and the oracle ledger.

## Typed records

Record written parameter/return/variable annotations, interface/type-alias/enum declarations,
class instance fields and constructor parameter properties, call-result bindings, type names,
and static heritage. Type aliases expose object members only when the whole aliased type is
an object literal; do not expand inferred/intersection/union shapes into invented fields.
Methods remain blocks; static members are not instance-field records.

Link each eligible type name, including generic arguments, to one same-language declaration.
Keyword/literal types, `this`, `typeof` queries, import types, and lexical type parameters
are not ordinary declaration links. Respect shadowing, type-only imports, qualified names,
merging ambiguity, builtin/library origins, and written heritage.

Types remain written text: do not substitute generic arguments or infer missing annotations.
`extends` and `implements` are distinct, and dynamic class bases do not supply static heritage.

For argument pairing, omit explicit `this` from runtime positions, preserve written argument
slots/expansions, and prove receiver binding before pairing. Transparent result wrappers include
parentheses, `await`, non-null assertions, and type assertions. See [pairing](../graph-model.md#pairing).

## Known limits and checks

There is no TypeScript type checker in the engine. Do not turn structural or annotation evidence
into checker-level inference, conditional-type evaluation, arbitrary object flow, dynamic
module loading, or runtime decorator/wrapper execution.

Start with relevant tests in `crates/unfold-lang-typescript/tests/structure/` and
`testdata/fixtures/typescript-evidence/`. Syntax changes need census/ranges/keys validation;
resolution changes need evidence and precision checks; module changes need configuration,
export, emit, and unindexed-target cases. Run the matching detector cases for detector edits.

Oracle policy lives in [testing](../testing.md#precision-and-recall-gates). A deliberate
change needs a specific justified entry, not a lowered recall floor or blanket exception.
