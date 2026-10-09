# TypeScript

This document describes how `crates/unfold-lang-typescript` turns TypeScript files into blocks,
slots, values, and occurrences, how it reads project configuration and resolves modules and
exports, and how it binds names and supplies the evidence the shared policy resolves edges from.
It carries over the proof of concept's native TypeScript front end
(`frontend/typescript-shared/{mapping,names,syntax,semantics,modules}.ts`, `frontend/native/`,
`source/{configuration,emit}.ts`, and the "Native front end", "Block selection", "Roles", "Names",
"Slots", "Value policy", "Import blocks", "Configuration and emit map", "Module naming", and
"Provenance" sections of its `docs/compiler.md`) onto oxc's parser and resolver, with Unfold's byte
offsets. The shared rules (selection,
keys, slot cardinality, validation) are in [the graph model](../graph-model.md); this file covers
what is specific to TypeScript.

Status: structure, project configuration, modules, exports, the emit map, module labels, the
generated library tables, binding, and evidence are complete. Every edge of the TypeScript
playground and the entrance fixture equals the proof of concept's native reference, apart from the
fixed defects C-2 and C-8 ([Parity](#parity)). The five entrance detectors (`package.script`,
`package.bin`, `bullmq.worker`, `express.route`, `node-cron.schedule`) are complete; their rules are in
[entrances](../entrances.md#typescript-detectors).

## Files and parsing

- A file is TypeScript when its name ends in `.ts`, `.tsx`, `.mts`, or `.cts` and TypeScript is
  graphed (`languages` in `.unfold/config.toml`). Declaration files (`.d.ts`, `.d.mts`, `.d.cts`)
  never get blocks: the language does not own them, and extracting one fails. JavaScript is V2.
- The parser is oxc (`oxc_parser`, `oxc_ast`, `oxc_ast_visit`, `oxc_span`, `oxc_syntax`,
  `oxc_allocator`), pinned exactly to **0.153.0** in the root `Cargo.toml` (U-7). Parentheses are
  kept in the tree, and the parser's token stream is collected for the census.
- `.ts` and `.tsx` files are modules when they use module syntax and scripts otherwise (as the proof
  of concept decided); `.mts` files are modules and `.cts` files CommonJS. `.tsx` files read JSX.
- Each file is parsed on its own; extraction never reads another file, and nothing is executed. The
  only inputs besides the bytes are the file's `verbatimModuleSyntax` ([Import blocks](#import-blocks))
  and its target-dependent binding semantics ([Binding](#binding)), which the engine asks for before
  extracting (`Language::extraction_options`, from the file's nearest `tsconfig.json`;
  [Project configuration](#project-configuration)). They are `ExtractOptions` fields and part of the
  facts-cache key, so a configuration change re-extracts exactly the files whose values changed.
  Module detection and the JSX mode apply when linking, so they never force extraction.
- Extraction converts oxc's AST once into a normalized tree that follows the proof of concept's
  syntax (categories, child slots, and token-exact ranges), drives the engine's shared selector over
  it, records values and occurrences, binds the file over oxc's tree ([Binding](#binding)), and records
  the payload and the census.
- Ranges are UTF-8 byte offsets, token-exact: they exclude leading comments, indentation, and
  trailing whitespace. Where oxc's spans differ from the proof of concept's, the range is rebuilt
  from the token stream: a declaration under `export` or decorators starts at the first of them, a
  computed name includes its brackets, a heritage slot starts at `extends`, and a switch's case
  block spans its braces even when empty.
- A byte-order mark stays in the text (the file root covers it). oxc is given the text with the mark,
  and a shebang right after it, blanked to spaces, so offsets do not move.

## Failures

A file fails, keeping no blocks, when:

- oxc reports any diagnostic. Parsing is strict: the message and byte offset of the first diagnostic
  (its primary label) become the failure, and a recovered tree is never used. This includes the
  grammar errors oxc checks while parsing (for example, a `this` parameter on a constructor).
- It nests too deeply. oxc parses recursively without a limit of its own, so each parse runs on a
  thread whose stack is sized from the text: every level of recursion consumes at least one token,
  and oxc's frames stay under 4 KiB per level, so 64 MiB plus 4 KiB per possible token is always
  enough. When that bound passes 1 GiB (roughly a megabyte of source), a lexical estimate of the
  nesting (brackets plus runs of recursive operators and keywords) must stay within 4,000. The
  conversion to the normalized tree and the census walk are depth-guarded at 4,000 levels, and the
  shared selector at 1,024 nested blocks; deeper files fail with a message.
- The file is not valid UTF-8 or is a declaration file.

Extraction never panics, whatever the bytes; arbitrary text, truncations of real files, and deep
nesting of every shape are tested.

## Syntax mapping

Selection follows the proof of concept's TypeScript mapping. Type syntax (annotations, type
arguments and parameters, interfaces, type aliases, `implements`, index signatures, and every type
node) is skipped with its whole subtree. Everything else that selects no block is traversed, so
calls beneath namespaces, enums, overload signatures, ambient declarations, parameters, and
brace-only blocks still become blocks.

| Syntax | Block | Slots |
| --- | --- | --- |
| Source file | `file` (the whole text) | — |
| Function declaration or expression, arrow function, with a body | `function` | `name`, `parameter[i]` (an explicit `this` parameter included), `return_type`, `body` (a block, or an arrow's expression) |
| Class declaration or expression, unless ambient (`declare`, or inside an ambient declaration) | `class` | `name`, `heritage` (the first `extends` clause, from `extends` through the base's type arguments) |
| Method, constructor, getter, setter, in a class or an object literal, with a body | `method`, `constructor`, `getter`, `setter` | as `function`; a constructor has no `name` slot |
| Variable declarator (statements, loop heads, catch bindings), unless ambient | `variable` | `name` (an identifier or the whole pattern), `initializer` |
| Binding element that binds one identifier in a variable's pattern | `variable` (named by the identifier) | `name`, `initializer` (its default) |
| Call | `call` | `callee`, `argument[i]` |
| Tagged template | `call` | `callee` (the tag) |
| `new`, with or without parentheses | `new` | `callee` (without type arguments), `argument[i]` |
| Runtime import, `export … from`, `import x = require(…)`, dynamic `import()` | `import` | `specifier` (the literal, or a dynamic import's first argument) |
| `if` | `if` | `condition`, `then`, `else`; an `else if` is a nested `if` with role `else` |
| `for` | `for` | `initializer` (a declaration list from its keyword, or an expression), `condition`, `update`, `body` |
| `for…in`, `for…of`, `for await…of` | `for_in`, `for_of` | `initializer`, `iterable`, `body` |
| `while`, `do…while` | `while`, `do_while` | `condition`, `body` |
| `switch` | `switch` | `expression`, `body` (the case block's braces) |
| `case`, `default` clause | `case`, `default` | `condition` (`case` only), `body` (first statement through last; none when empty) |
| `try` | `try` | `body`, `catch` (the clause from `catch`), `finally` (the block) |
| `catch` clause | `catch` | `binding` (the declaration, without parentheses), `body` |
| `return`, `throw` | `return`, `throw` | `expression` |
| `break`, `continue` | `break`, `continue` | — |
| Expression statement, including a directive such as `"use strict";` | `expression_statement` | `expression` |

Parameters bind no variables, so destructuring in a parameter selects no binding blocks. A
destructured declaration is an unnamed `variable` with one `binding` variable per bound name (D-18);
a binding element's range includes its property name and default, so `b: c` is named `c` and spans
`b: c`. Statement ranges include their semicolon when one is written; automatic semicolon insertion
ends them at their last token.

**Roles** use the first child slot on the path that leaves the nearest saved ancestor, across
unselected wrappers; a path through a decorator gets `decorator`, overriding everything else.

| Saved parent | Child slot → role |
| --- | --- |
| `file` | statements → `body` |
| function-like | body → `body`, parameters → `parameter` |
| `class` | members → `body`, heritage → `heritage` |
| `if` | condition → `condition`, then → `then`, else → `else` |
| `for` | initializer → `initializer`, condition → `condition`, increment → `update`, statement → `body` |
| `for_in`, `for_of` | initializer → `initializer`, expression → `iterable`, statement → `body` |
| `while`, `do_while` | condition → `condition`, statement → `body` |
| `switch` | expression → `expression`, case block → `body` |
| `case`, `default` | condition → `condition`, statements → `body` |
| `try` | try block → `body`, catch clause → `catch`, finally block → `finally` |
| `catch` | declaration → `binding`, block → `body` |
| `return`, `throw`, `expression_statement` | expression → `expression` |
| `variable` | initializer → `initializer`; a pattern name → `binding` |
| `call`, `new` | callee → `callee`, arguments → `argument` |
| function-like or `class` | a computed name → `name` |

Every other relationship has no role: a tagged template's tag and template, a binding element's
property name, and everything under an `import` block (such as a call in `import(path())`).

## Names

Names come from syntax, never from resolution.

- A declaration is named by its identifier, its private name (with the `#`), or its decoded string
  or numeric literal (numbers are spelled as JavaScript spells them: `0x10` names `16`). A computed
  name gives no name. Constructors are named `constructor`.
- A variable is named by its identifier; a destructured declaration has no name.
- An anonymous function, arrow, or class takes, after transparent wrappers (parentheses, `!`, `as`,
  `satisfies`, `<T>` assertions), the name of what it initializes: a variable, parameter, binding
  default, class field, or object property (when that name is not computed); or the identifier an
  `=`, `&&=`, `||=`, or `??=` assigns it to (including a default in a destructuring assignment); or
  `default` under `export default`. Otherwise it is unnamed.

## Value shapes and labels

Every call, construction, and tagged template records a `CallFact`: its callee value (the tag, for
a tagged template), its edge (`constructs` for `new`, otherwise `calls`), its context (`construct`
for `new` and for `super(…)`, which keeps its `call` block and `calls` edge), and its arguments in
written order. Every argument advances the position; a spread argument cannot supply a named
reference. Tagged templates have no arguments.

Values are recorded for callees, arguments, and the receivers, callees, and tags they depend on,
dependencies first. Parentheses, `!`, `as`, `satisfies`, and `<T>` assertions are transparent; an
optional chain is an ordinary access.

| Unwrapped expression | Shape | Label |
| --- | --- | --- |
| Identifier | `Name` | its text |
| Property access (`a.b`, `a?.b`, `a.#b`) | `Member` (receiver and member recorded) | `receiver.name` |
| Element access with a string, numeric, or plain template key | `Member` (receiver recorded) | `receiver["key"]`, as written (the key itself must be the literal for the label to keep it) |
| Other element access | `Other` (receiver recorded) | `receiver[…]` |
| Call, or dynamic `import()`, used as a value | `CallResult` | `callee()`, or `(…)()` |
| Function or arrow expression | `InlineFunction` (its block recorded) | `(…)` |
| Class expression | `InlineClass` (its block recorded) | `(…)` |
| `new`, tagged template | `Other` | the constructed expression's or the tag's label |
| `this`, `super` | `Other` | the keyword |
| Anything else (literals, spreads, operators, …) | `Other` | `(…)` |

Labels are joined by `unfold_engine::edges::text` and stay within 120 UTF-16 units.

The payload (`unfold_lang_typescript::payload`, postcard-encoded, versioned by `PAYLOAD_VERSION`,
now 4, which is part of the facts-cache key) records, aligned with the value table, what each value
is (an identifier and its range, a literal key, a call result and its callee, a construction,
`this`, `super`, ...), the kind of each runtime import (`Import`, `Reexport`, `ImportEquals`,
`Dynamic`), the module facts ([Exports](#exports)), the file's binding (`payload::Binding`;
[Binding](#binding)), the census, and the argument syntax entrance detectors read (`payload::Site`,
by block: decoded string literals in the first two argument positions, a route registration's
flattened handler arrays with their holes and spreads, and a construction's `new` second argument as
written; only for a `new`, a call of a name, or a call of a member named like a registration, and only
when something beyond the values is recorded). Besides callees, arguments, and their dependencies, the
value table holds the callee of every non-ambient `const` initialized by a call or `new` (a factory,
which provenance traces) and every element of a recorded handler array. The normalized syntax tells
array literals (`Array`) and their holes (`Hole`) apart from other expressions for this.

## Import blocks

Each runtime module dependency is one `import` block with one `imports` edge; an import
declaration never makes a call edge. Type-only subtrees contribute no calls.

| Syntax | Runtime dependency |
| --- | --- |
| `import "x"`, or an import with a default or namespace binding or a value specifier | Yes |
| `import type …`, `export type … from`, `export type * from` | No |
| Named `import { … } from` or `export { … } from` whose specifiers are all type-only, or none (`import {} from "x"`) | Only with `verbatimModuleSyntax` (fixes C-12) |
| `export * from`, `export * as ns from` | Yes |
| `import x = require("x")` | Yes, unless `import type`; `import x = A.B` is not a module |
| Dynamic `import(…)` | Yes; only a direct string or plain template literal is its specifier, so `import(("x"))` and `import(path())` are non-literal |
| Export of a local declaration or name | No |

The specifier label is the decoded literal. `verbatimModuleSyntax` comes from each file's nearest
`tsconfig.json` (false without one, or as `TypeScript::with_options` sets it).

## Census

Validation compares every TypeScript file with an independent census taken from the same parse:

- **Token boundaries** come from oxc's token stream (not from the normalized tree): every block and
  slot range must start at a token's first byte and end right after a token's last byte.
- **Call sites** come from a separate walk of oxc's AST that skips type syntax: every call, tagged
  template (`call`, `calls`), and construction (`new`, `constructs`), and the positions of spread
  arguments, which can never supply a reference. A dynamic `import()` is never a call site.
- **Module dependencies** come from the same walk, by the rules above.

## Highlighting

`TypeScriptHighlighter` gives the Code view lexical colors, as the proof of concept's reader did
(`reader/core/syntax.ts`): tokens and a few neighbor rules, never a semantic claim. Tokens come from
oxc's token stream with the program's comments and shebang; text that does not parse as TypeScript
is tried as TSX, and text oxc cannot get through at all (such as a slice cut out of a class body)
uses a small built-in scanner that never fails.

| Token | Class |
| --- | --- |
| Comment, shebang | `comment` |
| String, plain template, regular expression | `string` |
| Template head, middle, tail | `string`, with `${` and `}` as `keyword` |
| Number | `number` |
| Reserved word, not after `.` | `keyword` |
| Word before `(` or after `function` | `function` |
| Word after `.` or `?.`, not before another `.` | `property` |
| Word before `:` after `{` or `,`, directly inside braces | `property` |
| Other TypeScript keyword (`let`, `async`, `type`, `string`, …) | `keyword` |
| `PascalCase` word with a lowercase letter | `type` |

Adjacent spans of the same class merge; spans never overlap and lie on character boundaries.

## Project configuration

Each TypeScript file uses the `tsconfig.json` in its folder or the closest ancestor folder inside the
open folder (`unfold_lang_typescript::config`). Other `tsconfig*.json` files matter only when a chain
extends them. A file with no configuration uses ESNext and NodeNext, as the proof of concept did.
Unlike the proof of concept, the search never leaves the open folder: Unfold reads nothing outside it.

- **JSONC.** `//` and `/* */` comments, trailing commas, and a byte-order mark are accepted; anything
  else must be JSON. Duplicate keys keep the last value. Option names and enumerated values are
  case-insensitive. Unknown options are ignored; a known option with a wrong-typed value is reported
  and ignored.
- **Extends.** `extends` is a string or an array; later entries override earlier ones and the file's
  own options override them all (options merge one by one; `files`, `include`, and `exclude` are
  replaced whole). A path written `./x`, `../x`, or absolute names a file, with `.json` added when the
  path itself doesn't exist. Anything else names a package, looked up in `node_modules` from the
  configuration's folder up to the open folder: the exact file, then with `.json`, then the package's
  `"tsconfig"` field, then its `tsconfig.json`. `node_modules` is never walked: the language asks the
  engine for these candidates (`Language::requested_inputs`, role `tsconfig`), and the engine reads
  the ones that exist, round by round.
- **Problems, not failures.** A base that isn't found (with a hint when a package isn't installed), a
  circular chain, a chain deeper than 32 files, a base outside the folder, or a file that isn't valid
  JSONC is an input problem (`Language::input_problems`): it is listed, the graph is incomplete, and
  the configuration carries on with what it could read. The proof of concept stopped the whole build
  on any of these.
- **Paths.** `baseUrl`, `paths`, `rootDir`, `outDir`, `outFile`, and `declarationDir` resolve against
  the configuration that wrote them; `${configDir}` means the folder of the configuration being
  built. `paths` substitutions resolve against `baseUrl` when it is set, and otherwise against the
  configuration that wrote `paths`. Paths outside the open folder are dropped.
- **Inputs.** Every file of a used chain (the nearest configuration of any TypeScript file with facts,
  and its bases) is a graph input with role `tsconfig` (`Linked::inputs_read`). Any input change
  relinks; only files whose extraction options changed are extracted again.

| Option | Used for |
| --- | --- |
| `verbatimModuleSyntax` | Extraction: whether empty and type-only named imports stay ([Import blocks](#import-blocks)) |
| `module`, `moduleResolution`, `customConditions`, `allowJs`, `resolveJsonModule`, `baseUrl`, `paths` | [Module resolution](#modules-and-runtime-imports) |
| `moduleDetection`, `jsx` | Whether a file is a module ([Exports](#exports)) |
| `allowSyntheticDefaultImports` | Whether a missing `default` import falls back to `export =` (default true, as in the proof of concept's fork) |
| `target`, `useDefineForClassFields` | Binding semantics (`BindingContext`): parameter scope from ES2015, lowered optional chains below ES2020 and object rest below ES2017, standard class fields from ES2022 unless `useDefineForClassFields` is false; an unset or ES3 target is the latest standard |
| `rootDir`, `outDir`, `outFile`, `noEmit`, `emitDeclarationOnly`, `files`, `include`, `exclude` | The [emit map](#emit-map) only |

`moduleResolution` defaults from `module`: the Node kinds (`node16` through `nodenext`) resolve like
NodeNext, `commonjs` like node10, and everything else like a bundler, as TypeScript 6 does. Module
detection defaults to `force` for the Node module kinds (as the proof of concept's fork does) and to
`auto` otherwise. A configuration's file selectors never change what is graphed.

## Modules and runtime imports

Module resolution (`unfold_lang_typescript::modules`) uses `oxc_resolver`, pinned exactly (11.24.3),
over an in-memory view of the folder: the program's files, the unindexed files, the declaration files
discovery found (`ProjectInputs::context`), and the configuration inputs, with only `package.json`
contents readable. It never touches the disk, the network, or installed packages, so a resolution is a
function of fingerprinted inputs. TypeScript's rules, with the file's nearest configuration:

1. A relative specifier (`./x`, `../x`, `.`) resolves against the file's folder. An absolute one names
   nothing.
2. Any other specifier tries, in order: the `paths` pattern it matches (an exact pattern first, then
   the wildcard pattern with the longest prefix), each substitution in turn; `baseUrl`; the nearest
   `package.json`'s `imports` (`#x`) and a package's reference to itself; and an in-repository package
   whose `name` matches (through its `exports`, or its `types`/`typings`/`main` and files).
3. A path resolves by TypeScript's extension rules: `.js` and `.jsx` name `.ts`, `.tsx`, or `.d.ts`;
   `.mjs` and `.cjs` name `.mts`/`.d.mts` and `.cts`/`.d.cts`; a written TypeScript extension is used as
   is; outside ECMAScript-module mode an extensionless path tries `.ts`, `.tsx`, `.d.ts`, then a
   folder's `package.json` fields and `index`. JavaScript resolves only with `allowJs`, JSON only with
   `resolveJsonModule` and its extension written.
4. Under Node resolution the mode follows the import: an `import` or `export … from` uses the file's
   module format (`.mts` is ESM, `.cts` CommonJS, a `.ts` or `.tsx` file follows its nearest
   `package.json`'s `"type"`), `import x = require(…)` is CommonJS, and a dynamic `import()` is ESM.
   ESM mode never probes extensions or `index`. Package `exports` and `imports` match the conditions
   `types`, `node`, and `import` or `require`, plus `customConditions`; bundler resolution uses `types`
   and `import` or `require`; node10 ignores `exports` and `imports`.

| Resolves to, or fails as | Outcome (`ModuleTarget`) |
| --- | --- |
| A program file with facts | `resolved` to its file block |
| A program file that never parsed | `not_indexed` |
| An existing repository file outside the program: a source the scope leaves out, a declaration file, JavaScript with `allowJs`, JSON with `resolveJsonModule` | `external`, labeled with its repository-relative path (as the proof of concept labeled them) |
| A relative specifier naming nothing a module can be, while something exists there or at a probed variant (`x`, `x.js`, `x/index.ts`, ...) outside the program, such as JavaScript without `allowJs` | `not_indexed` (the proof of concept said `not_found`) |
| Any other relative or absolute specifier, or a matched `paths` pattern, naming nothing | `not_found` |
| Any other specifier | `external`, located at its package (`express/router` is in `express`, `@scope/name/x` in `@scope/name`), whether or not it is installed |
| A non-literal dynamic `import()` | `dynamic` |

Node built-ins are packages like any other (`fs`, `node:fs/promises`); only their labels differ
([Module labels](#module-labels)). A specifier whose label would be empty or start with `[` cannot be
a package, so it is `not_found`. Which syntax is a runtime import is in [Import blocks](#import-blocks);
`Linked::module_dependencies` answers one outcome per import fact, in order.

## Exports

Extraction records each file's module facts in the payload (`payload::ModuleFacts`): its module-scope
declarations (index 0 is the file itself; a `DeclRef`'s index is a position in this list), its import
bindings (local name, imported name, specifier or entity name, type-only, `export import`), its export
entries as written, whether it has module syntax, and whether it contains JSX. Module scope is the
top-level declarations plus `var` declarations hoisted out of top-level blocks, loops, conditionals,
and `try` statements; `declare global` and `declare module "…"` bodies are not module scope. Each
declaration names its block (found through the block's `name` slot, or by kind and end for anonymous
defaults and inline initializers) and the block a manual locator names: a function or class body, or
the function or class a `const` is initialized with.

Whether a file is a module follows its configuration's module detection: always with `force`; with
`legacy` when it has module syntax; with `auto` also when its format is ESM (`.mts`, `.cts`, or a
`"type": "module"` package under Node resolution) or it uses JSX with `react-jsx`/`react-jsxdev`.
Scripts export nothing.

`exports::ExportTable` builds every module's export table at link time, as the proof of concept's
native export queries did:

- A declaration written with `export` exports its name, with every declaration of that name (merged
  functions, classes, namespaces, enums, and overloads); `export default` exports `default` only.
  `export { local as name }` and `export import name = …` export the local name's declarations or the
  import binding's target. `export default <expression>` and `export = <expression>` follow an access
  path (`a.b["c"]`); any other expression is its own declaration.
- `export { a as name } from "m"` is the module's export `a`; `export * as name from "m"` is the module
  itself (its file declaration). `export * from "m"` adds every export of the module except `default`
  and `export=`, unless the file exports that name explicitly; stars that export different
  declarations under one name make it ambiguous.
- An import binding's target is the imported module's export: `*` is the module itself, `export=`
  falls back to the module, and a missing `default` falls back to `export=` unless
  `allowSyntheticDefaultImports` is false. A module with `export =` exposes the members of what it
  assigns. A module outside the program is external with its location; a source the scope excludes,
  or a file that never parsed, is an excluded boundary at its path.
- Type-only links, and names whose declarations are all types (interfaces, type aliases, namespaces
  without runtime declarations), are type-only and left out of `Linked::module_exports`. Cycles stop
  at the first repeated module and name.

`Linked::module_exports` lists the runtime exports sorted by name, each with its candidates (sorted
by file, start, and end; namespaces, interfaces, and type aliases are augmentations). Manual locators
use them first ([Linking](#linking)).

## Emit map

The emit map (`emit::EmitMap`, `Linked::emitted_source`) maps compiled outputs back to sources, as the
proof of concept's emit map did, without emitting anything or needing the outputs to exist. Every
distinct nearest `tsconfig.json` of an indexed TypeScript file contributes, in path order, unless it
sets `noEmit` or `emitDeclarationOnly`:

1. Its members are the folder's TypeScript files (and JavaScript files with `allowJs`) that its own
   `files`, `include`, and `exclude` select. Without `files` or `include`, `include` is `**/*`; without
   `exclude`, it is `node_modules`, `bower_components`, `jspm_packages`, and the `outDir`. A selector
   whose last component has no extension or wildcard names a folder; `*` and `?` stay within one
   component, `**` spans folders, and wildcards never match a name starting with `.`. Declaration files
   contribute nothing.
2. A member's output is its path relative to `rootDir` (the configuration's folder when unset, as in
   the proof of concept's fork) inside `outDir` (beside the source when unset), with `.ts`/`.tsx`
   becoming `.js` (`.jsx` for `.tsx` with `jsx: preserve`), `.mts` `.mjs`, and `.cts` `.cjs`. With
   `outFile`, every member's output is that file. A member outside `rootDir`, or an output outside the
   folder, has none.
3. An output two different members produce is ambiguous for good.

Members include sources the index leaves out (excluded by the scope, or failed): they make outputs
ambiguous, and an output only they produce leads to a source the launch reports as `not_indexed`
(the proof of concept dropped such mappings, so the launch fell through to `not_found`). The engine's
launch resolution asks the emit map before probing extensions, so on `ts-orders`,
`node dist/server.js` starts `src/server.ts` and the `orders` bin (`dist/cli.js`) starts `src/cli.ts`.

## Module labels

`labels` spells module labels for the shared rules (`unfold_engine::labels`):

| Written or located | Label |
| --- | --- |
| A Node built-in: `fs`, `fs/promises`, `node:fs`, `node:test` | `node:fs`, `node:fs/promises`, `node:fs`, `node:test` (subpaths kept) |
| Any other package specifier: `express/router`, `@scope/name/x` | `express`, `@scope/name` |
| A package location `@types/x`, `@types/a__b` | `x`, `@a/b` |
| The default library, or an undeclared global | `global` |
| A library type of Node's `NodeJS` namespace (`NodeJS.Process`), as a receiver | `node` (located at `@types/node`; C-6) |
| A repository file outside the program | Its repository-relative path |

A specifier is a package specifier unless it is empty, relative (starts with `.`), or absolute (`/`,
or a drive such as `C:\`). An import edge to a package is labeled from its specifier; one to a
repository file from its path. `global` applies only when the access root is undeclared and
unimported (C-6); that test is the shared rule's, on the evidence binding supplies (the root is
indexed when binding finds a declaration for it). A receiver typed with a library type is located
at the default library, except the types of Node's `NodeJS` namespace, which only `@types/node`
declares: a local parameter `process: NodeJS.Process` makes `process.cwd()` external `node`, never
`global` (the proof of concept said `global`, C-6).

## Generated tables

`cargo xtask gen-lib-tables <lib-dir> --node-types <dir> [--check]` parses TypeScript's `lib.*.d.ts`
files and `@types/node` (its `index.d.ts` and every file it references) with oxc and writes
`crates/unfold-lang-typescript/src/tables/generated.rs`, with the source versions and a SHA-256 of the
declaration set in its header (TypeScript 6.0.3 with 107 lib files, and @types/node 25.3.3, the
versions the proof of concept generated from). Each table is a sorted static array searched by
bisection (`tables`):

| Table | Contents | Used for |
| --- | --- | --- |
| `is_known_global` | Global value names: top-level declarations of script files and `declare global` blocks with a value meaning (variables, functions, classes, enums, instantiated namespaces), plus `globalThis` and `undefined` | Evidence level 2: undeclared known globals and their member chains are external |
| `is_library_type` | Global class and interface names, qualified through namespaces (`Intl.Collator`, `NodeJS.Process`); a leading `globalThis.` is accepted | Evidence level 4: written library receiver types |
| `is_builtin_module` | Node built-in module specifiers, from the ambient modules `@types/node` declares (`fs`, `fs/promises`, `node:fs`, `node:test`) | Module labels |

The global and library-type tables equal the proof of concept's generated lists exactly (1,034 globals;
1,795 library types, whose `globalThis.`-prefixed duplicates the lookup strips). The built-in list has
111 specifiers; compared with Node's own list, it lacks Node's internal `_http_*`/`_tls_*` modules and
`sys`, which `@types/node` doesn't declare.

## Binding

Each file is bound once, at extraction (`bind`), with the proof of concept's binder rules
(`frontend/native/names.ts`, adapted from TypeScript's binder and `createNameResolver`) ported onto
oxc's tree: one walk builds the scopes and symbols, creates a declaration for everything evidence
can name, and converts every expression into the payload's compact form; names are looked up once
the walk has seen every declaration of the file. Linking never sees a syntax tree.

| Container | Scope |
| --- | --- |
| File | Module files keep their declarations; a script's file-scope symbols are global to its configuration group (the files sharing a nearest `tsconfig.json`), merged by name across files at link time |
| Function, method, constructor, accessor | One scope for parameters and body; rebinds `this` |
| Arrow | One scope for parameters and body; inherits `this` |
| Block, loop (`for`, `for…in`, `for…of`), catch clause, switch case block, static block | Their own scope |
| Class | The class's own name, type parameters, and member evaluation |
| Namespace | Each declaration has its own scope (its private locals), whose parent is the namespace's shared member scope (what every declaration exports), whose parent is where the namespace was first declared |
| Enum | The members, shared by every declaration of the enum |
| `with` statement body | No static lookup: names inside are `dynamic` |
| `declare global { … }` | The group's global scope |

- **Declarations.** `var` binds in the nearest function, arrow, static block, namespace, or file
  scope; `let`, `const`, `using`, classes, and function declarations stay in their block (also in
  scripts). Destructuring declares each identifier as a binding element of its variable or
  parameter. A named function expression binds its name inside itself; a named class expression
  inside its body; a class declaration both outside and inside.
- **Merging.** Every declaration of one meaning (value or type) merges into one symbol per name and
  scope; a declaration of either meaning joins an existing symbol found in the meanings it declares.
  So overloads, merged namespaces, enums, and interfaces are one symbol each, a namespace can augment
  a function, class, or enum, and an interface and a later value of the same name stay two symbols.
  Exported namespace members also enter the shared member scope.
- **Lookup.** A name is looked up from its scope outward in the meaning it is used in (a type
  annotation's names as types, `typeof` names as values). Decorators, computed member names, class
  heritage, and parameter decorators are evaluated in the surrounding scope. From ES2015 on, a
  reference inside a parameter list skips the callable's body variables, unless the parameter list
  uses syntax the target lowers into the body (optional chains and `??` below ES2020, object rest
  below ES2017, a static field of a class expression without standard class fields); this needs the
  target, so it is an extraction option. A name the file doesn't declare is resolved at link time:
  first among the other declarations' exports of the script-global namespaces around it, then among
  the group's script globals, then the known library globals.
- **`this` and `super`.** Each occurrence records its class and whether the member is static: a
  class method, constructor, accessor, property initializer, or static block binds `this`; arrows
  look further out; a function, object-literal method, the file, a namespace, or an enum leaves it
  unbound; a class member's computed key leaves `this` unbound while `super` looks further out.
- **Private names** resolve to the members of the nearest enclosing class that declares them.
- **Import aliases.** A module-scope import binding is linked through the export tables;
  `import x = A.B.C` follows `A` lexically and then its members; a binding destructured from a
  literal awaited dynamic import reads that module's export (C-8, [Decisions](#decisions)).

The payload's binding (`payload::Binding`) holds the declarations (the first ones are the module
declarations, in the same order, so a `DeclRef` index means the same declaration everywhere), the
symbols, the namespaces' and enums' member tables, the expressions evidence evaluates (only those a
value, initializer, class base, or member write reaches), the written types it reads, what each
`this` and `super` means, every class's members, the members of `const` object literals, and the
narrowing sites and member writes.

## Evidence levels

Linking answers the shared policy with the proof of concept's native evidence
(`frontend/native/index.ts`, `classes.ts`, `declared-types.ts`): no inference, no checker, and an
unknown constituent gives no answer. Every query starts afresh (with its own recursion guard and
memo) and reads tables built once per link, so answers never depend on query order or thread.

| Level | Evidence |
| --- | --- |
| 1 | Lexical functions; `const` function, arrow, and class initializers; immediately invoked functions |
| 2 | Named, default, and namespace imports; re-exports; packages; known library globals and their member chains (external `global`) |
| 3 | Classes, `new`, `super`, `this`, static members, inherited members |
| 4 | Written class and library annotations, explicit `new` and assertion initializers, parameter properties, property chains, declared return types; library receivers (`string`, arrays, `Map`, …) from the generated tables |
| C-2 | Members of constant object literals: `logger.info` |

What an expression denotes:

| Expression | Target |
| --- | --- |
| Identifier | Its symbol's declarations (an import's target through the export tables, as the proof of concept's callee linker followed it); a script global; a known library global (external); otherwise nothing |
| Property or literal-key access | A private name's members; else, if the receiver has a class, that class's member through its bases (an external or excluded base ends there); an external receiver's member; a constant object literal's function member (C-2); else a member of what the receiver denotes (a module's export or a namespace's member) |
| `super` | The nearest enclosing class's written base |
| `this` in a static member | The class |
| Inline function or class | Itself |
| Anything else | Nothing |

A receiver has a class, an external or excluded boundary, or nothing: an `as`/`<T>` assertion's
type; nothing if the receiver is narrowed; `this` and `super` from their context; a variable written
with a type annotation by its declared type; a class itself (static access); otherwise its declared
type. Declared types come from annotations of variables, parameters, properties, and getters, from
`new`/assertion initializers, from the declared return types of a callee's callables (overload
signatures included, when one has a body and all agree), and from class members' declared types
along property chains. A type alias without type parameters is followed; a local name shadows a
library type even when it is not a type. A receiver is **narrowed** when it, or a receiver it is a
member of, is the left operand of `instanceof` or an argument of a call to a type predicate (or of
any call of unknown kind inside a condition) anywhere in the program.

Evidence for a value: the target's external locations, excluded boundaries, and declarations as
candidates (namespaces, interfaces, and type aliases only augment); the receiver class (for
`this.m()`, the nearest enclosing class); static access (a static candidate); `super` access; an
unbound `this` at the access root; ambiguity when an instance method or field may be overridden by a
class whose base is unknown; the root's import binding; and whether the root is declared at all
(an undeclared, unimported root is labeled `global`).

Declaration answers: concrete bodies (a non-ambient function, method, or constructor with a body; a
`const` function, arrow, or class initializer; a class field's function or arrow initializer; for a
construction, a class's first constructor with a body or else the class; for a reference or
handler, the class); parameters (a binding element reports its whole parameter; the position is the
runtime argument position, so an explicit `this` parameter is skipped: C-4); stability (`const`
stable; `let`, `var`, and written object-literal members rebindable; catch bindings unknown);
accessors; class-member facts; overload signatures (same container, name, and kind, without a body,
not ambient; an abstract method counts); provenance facts (a non-ambient `const` factory's callee;
annotations of parameters, variables, and properties); and annotation evidence (a written type
reference's target, followed as a type).

## Classes and the override census

Every class (declaration or expression) has a shape: its declaration, its base, and its members
(constructors, methods, accessors, fields, `accessor` fields, then parameter properties). A base is
the class its `extends` expression denotes: directly, through a static receiver, or through
variables initialized with a class (an annotation stays authoritative). Bases are resolved once per
link. The engine's override census (`unfold_engine::resolve`) withholds an instance method or field
as `overridden` when an indexed descendant of the receiver's class declares a member that can
override it (not a constructor, not ambient, with a body or a field or parameter property; an
unknown computed name overrides anything); private names are their own slots, and `super` and
static access stay exact. A class whose base chain reaches an *unknown base* (an `extends`
expression naming nothing known, other than `null`) may be an indexed subclass of anything, so its
overriding members make calls of those names on other receivers `ambiguous`.

## Mutation census

The C-2 rule resolves `obj.member` when `obj` denotes exactly one non-ambient `const` whose value,
as written, is an object literal (no type annotation on the variable, no `as` or `<T>` assertion
around the literal; parentheses and `satisfies` are fine), the literal has no spread and no computed
key that isn't a literal, and every member of that name is a method with a body or a property whose
value is a function or arrow. Any other member of a constant object (a value, an accessor, a
shorthand property) is left to the other rules.

The mutation census records every write to a member, anywhere in the program: `obj.member = …` and
compound or logical assignments, `obj["member"] = …`, `++`/`--`, destructuring assignment targets,
and `delete obj.member`; a computed key writes every member. A written member of a constant object
literal is not concrete and its binding is rebindable, so a call to it is `mutable_binding`.

## Linking

`TypeScript::link` returns `link::TypeScriptLinked`, which answers module dependencies, exports, the
evidence and declaration questions above, class shapes, locators, module labels, the inputs it
read, the emit map, and the census, and exposes the module layer (`resolve`, `exports`,
`module_facts`, `is_module`, `binding_context`). Linking decodes every payload once and builds, in
order: the configuration groups and their script globals, the export tables (runtime exports, and
the same names followed as values and as types, as the callee linker followed them: type-only links
are skipped as values and followed as types, a name's every declaration is its target, and each
merged symbol is its own identity, so a star export of an interface and a value of the same name is
ambiguous), every class's members, the narrowed receivers, the class bases (in parallel), the
members that may override unknown bases, the mutation census, and the class shapes. It also reads the
`package.json` files the package detectors use (listed with `Linked::inputs_read`; an unreadable one is a
`Linked::problems` entry) and returns the five entrance detectors (`Linked::detectors`,
`src/detect/`).

Manual locators use `Linked::module_exports`, then `Linked::locator_top_level` (the functions,
classes, and variables that are direct statements of the file, under `export` too), then
`Linked::locator_concrete` (a function or method body, a constant's function, arrow, or class
initializer, or a class, constructor, or accessor). `Owner.member` (`Linked::locator_members`)
names a class's members (a constructor is `constructor`) or a constant object literal's members (a
function-valued property names its function).

## Known limits

- Evidence follows only what the syntax states. Inference, contextual typing (an untyped callback
  parameter such as Express's `res`), generic substitution, unions, and flow narrowing give no
  receiver, so those calls stay `unknown_receiver`.
- C-2 covers one level: a member of a constant object literal, not a member of an object nested in
  it, and not an object reached through another variable. The mutation census sees writes through
  the object's own name (or any expression denoting it), not writes through aliases, function
  arguments (`Object.assign`), or reflection.
- Only `NodeJS` namespace types are located at `@types/node`; other global types `@types/node`
  declares (such as `Buffer`) are labeled `global`, because the generated table does not record
  which declaration set a type comes from.
- A namespace import binding (`const ns = await import("./m.js")`) is not a namespace: only
  destructured properties of an awaited dynamic import resolve (C-8).
- Script-global namespaces merge across files for lookups inside them and for members; a nested
  namespace that is not exported is merged only within its file.

## Decisions

- **S2: binding is the proof of concept's binder ported onto oxc's tree, not `oxc_semantic`.** A
  spike ran `oxc_semantic` 0.153.0 over binder fixtures. It agrees on block-scoped functions,
  class-expression names, catch bindings, interface-and-value merging, and overloads, but it
  disagrees with the rules this front end must keep: a name exported by one declaration of a merged
  namespace is unresolved inside another declaration (`namespace A { export function f() {} }
  namespace A { f(); }`), and so are dotted namespaces (`namespace A.B {}` then `B` inside `namespace
  A`) and enum members across merged enums (`enum E { A } enum E { B = A }`); `declare global`
  declarations stay in a local scope; and the target-dependent parameter scope has no setting.
  Correcting those would mean re-binding on top of it, and evidence also needs facts it doesn't keep
  (meanings per declaration, `this` contexts, declaration containers), so the binder walks oxc's tree
  itself. Its fixtures (`testdata/fixtures/typescript-evidence/binder-*`, `scripts`) agree with the
  proof of concept's native producer edge for edge.
- **C-8: a binding destructured from a literal awaited dynamic import resolves.** In
  `const { printReport } = await import("./jobs/report.ts")` (a `const`, the literal specifier
  written directly, the `await` directly on the import, a plain property with an identifier or
  literal key and no default or nested pattern), the binding is the module's export of that name, as
  if written `import { printReport } from "./jobs/report.ts"`, and is followed the same way (an
  indexed module's export, or an external package). `let` bindings, defaults, rest elements, nested
  patterns, computed keys, non-awaited imports, and non-literal specifiers keep their ordinary
  answers. Both proof-of-concept producers left these calls `not_concrete`; the playground's
  `printReport()` is listed as a divergence (`testdata/fixtures/typescript-evidence/c8-dynamic-import`) and as
  the one justified oracle disagreement ([testing](../testing.md#precision-and-recall-gates)).
- **C-9: property locators and calls agree wherever a call has evidence.** A manual locator
  (`src/api.ts#api.arrow`) and a call (`api.arrow()`) of a constant object literal's function member name the
  same block through the C-2 fix; the remaining differences (annotated or mutated objects) are deliberate. The
  decision and its table are in [entrances](../entrances.md#c-9-property-locators-and-calls).

## Parity

`tests/structure/parity.rs` compiles `testdata/playgrounds/ts-orders` and
`testdata/fixtures/{worked,classes,entrances,playground-v1}` with each folder's configuration,
validates the graphs with the census, and compares them with
`testdata/reference/canonical/{playground-ts-native,entrances}.jsonl`: files, blocks, slots, and
every edge are identical (34 files, 676 blocks, 1,168 slots, 303 edges; and 5 files, 31 blocks, 46
slots, 15 edges), except 26 playground calls a fixed defect resolves: 25 C-2 calls (14 `logger.info`,
8 `logger.error`, `events.on`, `events.emit`, `slack.post`), each with the target the checker-backed
reference resolves, and the C-8 `printReport()`. Every resolved playground edge (72 calls and
constructions and 15 references) names the checker-backed reference's target, except the C-8 call,
which the checker leaves `not_concrete`; recall is 71 of the checker's 71 resolved calls (the native
producer reached 46). `cargo xtask parity playground-ts-native --scope files,blocks,slots,edges`
(and `entrances`, `mixed-playground`) pass with these divergences listed. With the detectors, all three
pass at full scope (entrances, targets, and the `package.json` input included);
`tests/entrances.rs` checks the playground's 25 entrances and 28 targets and every detector case.

`tests/structure/evidence.rs` pins the edges of every case under
`testdata/fixtures/typescript-evidence/` (the proof of concept's native binding, class, declared-type,
narrowing, receiver, value, and reference test fixtures, plus Unfold's binder and defect cases); its
`README.md` lists the deliberate differences from the native producer. Compared with the native
producer's own output, Unfold's edges agree on the proof of concept's 602-file corpus (108,885
edges) except four C-2 resolutions, and on its 186-file self corpus (20,424 edges) exactly; every
edge Unfold resolves there names the checker-backed producer's target, and recall on the 602-file
corpus is 70,720 of the checker's 73,649 resolved calls (96.02%). `tests/structure/determinism.rs`
checks that the canonical export is identical across thread counts and input orders, and an ignored
test does the same on the 602-file corpus. The syntax cases in
`testdata/fixtures/typescript-structure/` pin the mapping for syntax the playground does not use. An
ignored test parses and validates every file of the 602-file corpus (TypeScript 6.0.3's `src`).
