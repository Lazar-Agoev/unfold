# TypeScript front end

Read this for TypeScript parsing, lowering to nodes, project and module resolution, binding, or typed records.
Start at [lib.rs](../../crates/unfold-lang-typescript/src/lib.rs). The block model, keys, outcomes, and the shared
resolution policy are in [Graph model](../graph-model.md).

## Code map

| Concern | Owning module |
| --- | --- |
| Parsing and per-file extraction | `extract.rs`, `stack.rs`, `scan.rs` |
| Lowering to nodes | `lower/` (`define.rs`, `statements.rs`, `expressions.rs`, `forms.rs`) |
| Census | `census.rs` |
| Cached payload | `payload/`, `module_syntax.rs` |
| Project configuration and paths | `config/`, `paths.rs` |
| Modules and exports | `modules/`, `exports/` |
| Scope and name binding | `bind/` |
| Link-time evidence and answers | `evidence/`, `link.rs`, `values.rs` |
| Written types | `typed.rs` |
| Module labels | `labels.rs` |
| Library and ECMAScript tables | `tables/`; regenerate with `cargo xtask gen-lib-tables` |
| Highlighting for the code panel | `highlight/` |

Paths are relative to `crates/unfold-lang-typescript/src/`. Read a module's comment and a nearby fixture before
expanding to other stages.

## Files and parsing

`.ts`, `.tsx`, `.mts`, and `.cts` files are sources. Declaration files (`.d.ts`, `.d.mts`, `.d.cts`) are context:
they inform names and types but get no nodes. JavaScript files are not indexed; module resolution still sees them
(they resolve with `allowJs`, and are `not_indexed` otherwise).

Parsing uses the pinned oxc parser and is strict: any syntax diagnostic fails the file with its message and offset,
and a recovered tree is never used. Each parse runs on a thread whose stack is sized from the text, and text nested
deeper than the census's limit fails instead of overflowing. Extraction keeps token-exact ranges and never
pretty-prints syntax. Linking reads only cached facts, never a retained parse tree.

## Lowering

`lower/` turns oxc's tree straight into nodes, one rule per syntax form, keyed in preorder with `KeyBuilder`.

| Written | Node |
| --- | --- |
| The file | `module`; a leading file JSDoc is its `doc` |
| Function declarations; methods, constructors, accessors (class or object literal) | `function` (`function`, `method`, `constructor`, `getter`, `setter`) |
| Arrow functions, function expressions | `function` (`lambda`): `callback[n]` in a call's arguments, object properties included; named by `const x = …`, a class property, a property of an object a declaration names, or `export default`; `lambda[n]` anywhere else |
| `class`; `interface`, `type`, `enum` | `class`; `type` (`interface`, `type_alias`, `enum`) |
| Decorators | Calls in the enclosing body, role `decorator`, `for` the definition, no `stmt` |
| A class's `extends` call | A call in the enclosing body, role `base` |
| Parameter defaults | Calls at the start of the function's own body, role `default` |
| `if` / `else if` / `else` | `decision` (`if`): `then`, `elif[n]`, `else`; test calls have role `test` |
| `c ? a : b` with a call | `decision` (`ternary`) |
| `switch` | `decision` (`switch`), headed by its discriminant: `case[n]`, `default`; a case without a final `break`, `return`, `throw`, or `continue` before the next one `falls_through` |
| `for`, `for … in`, `for … of`, `for await`, `while`, `do … while` | `loop` (`for`, `while`, `do`) with `body`; header calls in the loop |
| `try` / `catch` / `finally` | `try` with `body`, `catch`, `finally` |
| `using`, `await using` | `with` (`using`): the declaration is its header, the rest of the block its `body` |
| `return`, `throw`, `break`, `continue`, `yield` | `exit`, as [Graph model › Exits](../graph-model.md#exits) describes; labels resolved |
| Calls, tagged templates, `new`, `import()` | `call` (`new` for `new`); `import()` also has an `imports` link |
| `&&`, `\|\|`, `??`, `?.` | Their calls are `conditional` |
| Declarations without a call, expression statements, imports, JSX, type syntax | Code: calls are nodes, imports are links, types are typed records |

Choices the lowering makes:

- Namespaces are not nodes; their definitions are qualified (`N.f`).
- A function expression's own name is ignored for keys; the declaration or property that holds it names it.
- A template literal without substitutions counts as a string literal, for callees (`""`) and for the
  first-literal key rule.
- A JSDoc is a definition's `doc` only with no blank line between it and the definition.
- A decorator on a class field is a plain call: a field isn't a definition.
- What can't be placed is marked `truncated` on the nearest node above it: nesting past the depth limit (the code
  below is left out and the census skips it), a `break` out of a labeled plain block, and a class expression with
  no name to key it by.

## Project configuration

Each file uses the `tsconfig.json` in its folder or the nearest ancestor folder inside the project; a file with
none uses ESNext and NodeNext. `extends` is a string or an array, later entries overriding earlier ones; a package
base is looked up in `node_modules` from the configuration's folder upward, read on request without walking
`node_modules`. A base that can't be found, a cycle, or a file that doesn't parse is an input problem, never a failed
update. Path options resolve against the configuration that wrote them (`${configDir}` is the extending
configuration's folder); paths outside the project are dropped. `files`, `include`, and `exclude` never change what
is graphed, so they aren't read.

Module resolution follows TypeScript's rules over an in-memory view of the project (its sources, unindexed and
context files, and configuration inputs), never the disk or installed packages:

1. A relative specifier resolves against the file's folder.
2. Otherwise `paths` (exact pattern first, then longest prefix), then `baseUrl`, then the nearest `package.json`'s
   `imports` and self-reference, then an in-repository package by `name` (through its `exports`, or its files).
3. Files resolve by TypeScript's extension rules (`.js` names `.ts`, `.tsx`, or `.d.ts`). JavaScript resolves only
   with `allowJs`; JSON only with `resolveJsonModule` and its extension written.
4. Under `node16`/`nodenext`, the mode follows the file's module format; `exports` and `imports` use `types`,
   `node`, `import` or `require`, and `customConditions`.

What the specifier reaches decides the import's outcome:

| Reaches | Outcome |
| --- | --- |
| An indexed file | That module |
| A program file that never parsed, or (relative) an existing file that isn't a resolvable module | `not_indexed` |
| A repository file outside the program: a source the scope excludes, a declaration, JavaScript, or JSON file | External, labeled with its path; calls through it are `not_indexed` |
| Nothing, for a relative, absolute, or `paths` specifier | `not_found` |
| Anything else | A package, external with its package name whether or not it is installed |

Export tables follow `export` declarations, re-exports, `export *` (explicit exports win; two stars that disagree
make a name ambiguous), `export =`, and `export default`, stopping at cycles. A module outside the program is
external, or an excluded boundary for a source that scope excludes or that failed.

Which module syntax is a runtime dependency (an `imports` link) is decided once, in `imports.rs`, for both
extraction and the census: type-only imports and exports aren't; named imports whose specifiers are all type-only
are only with `verbatimModuleSyntax`; a dynamic `import()` with a non-literal specifier is `dynamic`.

Implementation: [config/mod.rs](../../crates/unfold-lang-typescript/src/config/mod.rs),
[modules/mod.rs](../../crates/unfold-lang-typescript/src/modules/mod.rs),
[exports/mod.rs](../../crates/unfold-lang-typescript/src/exports/mod.rs).

## Binding and evidence

The binder (`bind/`) builds each file's scopes and symbols and a declaration for everything evidence can name:
functions, classes, members, variables, parameters, namespaces, enums, imports, the members of `const` object
literals, and inline functions and classes. It handles target-dependent parameter scopes, merged declarations and
namespaces, hoisting, catch and class scopes, type and value meanings, private names, and what `this` and `super`
mean at each occurrence.

Evidence comes from direct lexical bindings, imports through the export tables, script globals, known library
globals, namespace members, class members through receivers, and the function members of `const` object literals.
A stable alias keeps identity; a mutable or ambiguous binding never becomes a proven target because one assignment
looks callable. The write census notices alias and member writes and supported reflective mutations.

`this` follows member and arrow ownership; an unbound non-arrow `this` is `unbound_this`. `super`, static, private,
and instance accesses dispatch differently. The class and override census checks known descendants, unknown bases,
and computed member names: a method body alone doesn't prove which implementation a receiver reaches. A compatible
overload signature beside its implementation is one target.

A plain `const` destructuring of a directly awaited literal import binds the module's export:
`const { report } = await import("./report.ts")`. Defaults, rest, nested or computed patterns, mutable bindings,
non-awaited imports, and dynamic specifiers don't get this proof (fixture
`testdata/fixtures/typescript-evidence/c8-dynamic-import`, with a justified oracle entry).

## What a call's value is

A receiver's type comes only from what is written ([receivers.rs](../../crates/unfold-lang-typescript/src/evidence/receivers.rs)):

- annotations of variables, parameters, properties, and getters, and class members' declared types along a chain;
- explicit `new`, `as` and `<T>` assertions, and initializers (`new`, a call, or `await`) of a `const`, or of a
  `let`/`var` that is never assigned again; never a parameter's default;
- a class property's initializer, when nothing writes the property after its declaration (an assignment, `++`,
  destructuring, `delete`, `Object.defineProperty`/`assign`, a write with an unknown key, or a subclass member of
  the same name); an overwritten property is `mutable_binding`;
- a callee's declared return type, when every declaration agrees and one has a body. A class, or a class with
  `null` or `undefined`, gives that class;
- through `await`, the `Promise<Repo>` an `async` callee declares, with the library `Promise`.

Library receivers (`string`, arrays, `Map`, `NodeJS.Process`) come from the generated tables. **ECMAScript's
standard objects are `builtin ecmascript`:** its globals by name (`JSON.parse`, `Object.keys`, `String(…)`,
`new Error(…)`, `extends Error`), and a built-in method on a primitive, a literal, an exact value, or a written class
type (`[1, 2].map`, `rows.map` where `rows: Row[]`). A written class type is `overridden` when a project class that
may be one (through `extends`, or an `implements` naming the type) declares the member. Structural interfaces
(`Iterator`, `PromiseLike`, `ArrayLike`, `ReadonlyArray`), `Object`, `object`, and `Function` give no receiver. A
project write to a global, its member, or its prototype (`JSON.parse = …`, `Object.defineProperty(Array.prototype,
…)`) makes those calls `dynamic`. DOM and Node globals (`console`, `fetch`, `setTimeout`, `process`) stay
`library global`.

There is no inference, substitution, contextual typing, or flow narrowing. A receiver whose root is the left operand
of `instanceof`, or an argument of a type predicate (or of an unknown call in a condition), anywhere in the program,
gets no declared receiver. Generic, union-of-classes, and disagreeing return types leave the value unknown.

## Module labels

| Written or located | Label |
| --- | --- |
| A Node built-in: `fs`, `fs/promises`, `node:fs` | `node:fs`, `node:fs/promises` |
| A package specifier: `express/router`, `@scope/name/sub` | `express`, `@scope/name` |
| A package location `@types/x`, `@types/a__b` | `x`, `@a/b` |
| ECMAScript's standard objects | `ecmascript` (outcome `builtin`) |
| The rest of the default library, or a known global (DOM, Node) | `global` |
| An unindexed repository file | Its repository-relative path |

The tables are generated from TypeScript's `lib.*.d.ts` files and `@types/node` at pinned versions; the ECMAScript
tables come from the standard editions, `lib.es5` through `lib.es2025`, not `lib.esnext`. Their versions and hashes are in the generated files' headers
([tables/mod.rs](../../crates/unfold-lang-typescript/src/tables/mod.rs)).

## Typed records

Recorded: parameter and return annotations, the members of interfaces, enums, and classes (instance fields,
`accessor` properties, and constructor parameter properties; static members are not), the members of a type
alias whose whole aliased type is an object type literal, and the type names of those written types, variable
annotations, catch clause annotations, and heritage clauses. Intersections, unions, and inferred shapes are never
expanded into fields. Methods are nodes.

Each type name links to one same-language declaration, generic arguments included. Keyword and literal types,
`this`, `typeof x`, `import("m").T`, and lexical type parameters are not links. A class's `extends` names a type
only when it is a name or member chain (`mixin(Base)` names none); `extends` and `implements` stay distinct. Types
stay as written: no generic substitution, no inferred annotations.

For pairing, an explicit `this` parameter takes no argument, and parentheses, `await`, `!`, and type assertions are
transparent for where a value lands. See [Pairing](../graph-model.md#pairing).

## Known limits and checks

There is no TypeScript type checker in the engine. Structural and annotation evidence never turns into checker-level
inference, conditional-type evaluation, arbitrary object flow, dynamic module loading, or running decorators or
wrappers.

Tests are one binary, `crates/unfold-lang-typescript/tests/structure/` (lowering, configuration, evidence,
resolution, typed records, robustness, determinism, and the playground goldens). Fixtures:
`testdata/fixtures/typescript-structure/` (expected node trees), `typescript-evidence/` (expected links), and
`typescript-typed/`. Syntax changes need lowering, census, range, and key cases; resolution changes need evidence
and precision checks; module changes need configuration, export, and unindexed-target cases.

The precision gate compares resolved calls with the TypeScript checker's answers ([Testing](../testing.md)). A
deliberate disagreement needs a specific justified entry, never a lowered recall floor or a blanket exception.
