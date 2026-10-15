# TypeScript evidence cases

Each folder is a small TypeScript project (`tsconfig.json`: ESNext, NodeNext, no `types`, unless the
folder has its own) and an `expected.links` golden: one line per link, `from #seq kind `text` →
outcome`, where the outcome is `project <key>`, `library <module>`, `builtin <module>`, or
`unknown <reason>`. The links
are the calls (`calls`, and `passes` for a definition passed as an argument), the written module
dependencies (`imports`), and the type names of annotations and heritage clauses (`types`, and
`extends` flagged `(implements)` for an implemented interface).
`crates/unfold-lang-typescript/tests/structure/evidence.rs` compiles every folder and compares;
`UNFOLD_BLESS=1` rewrites the goldens.

- **`poc-<suite>-<n>`**: the inline fixture sources of the proof of concept's tests
  `compilers/codegraph/test/<suite>.test.ts` at commit `5c98a6e` (native binding, classes, declared
  types, narrowing, array and library receivers, schema 2 values, references, and native cases), in
  the order they appear there.
- **The other folders** cover binder rules (`binder-*`), the override census
  (`overrides`), overloads, narrowing, receivers, script globals across files (`scripts`), the C
  decisions (`c2-objects`, `c4-this-parameter`, `c6-node-process`, `c8-dynamic-import`), written return
  types (`return-types`: a class or nothing, a generic type by its class, `await` on `async` callables, and call
  initializers), initializers of reassigned variables (`reassigned`) and overwritten class properties
  (`class-fields`), which aren't followed, and ECMAScript's standard objects (`builtins`: its globals by name, and a
  built-in method on a literal or a written class type, beside host globals, which stay `library global`;
  `builtins-overrides`: a written class type a project class may override is `overridden`, and structural
  interfaces give no receiver; `builtins-inherited`: a method a project class inherits from an ECMAScript base is a
  builtin, and a member inherited from outside the project is `overridden` when a subclass declares it;
  `builtins-patched`: a project write to a global or a prototype makes its calls `dynamic`, inherited ones included;
  `builtins-instances`: so does a write to an instance's member, for every value of its type; `builtins-replaced`:
  so is every member of what `new` of a replaced global constructs; `builtins-members`: `Intl`'s values and globals
  read from `globalThis` are builtins, and a call through a member of a library value is `unknown_receiver`, while
  the member itself keeps its label). In `class-fields` the
  checker resolves `Swapped`, `Exposed`, and `Base` to `Repo.save` by declared type, where Unfold, which can't prove
  the property's value, leaves them unknown.

Every call a golden resolves has the TypeScript checker's target where the checker resolves it; the
only resolved calls the checker leaves unresolved are the C-8 ones below. Against the proof of
concept's native producer (`index --frontend native` on a copy of the folder) the goldens' calls,
passed definitions, and imports agree link for link, except that ECMAScript's standard
objects (its globals by name, their methods, a built-in method on a written type, and type names
such as `Promise`) are `builtin ecmascript` where the native producer says external `global`, and
for these deliberate differences:

| Folder | Edge | Native producer | Unfold | Why |
| --- | --- | --- | --- | --- |
| `poc-native-declared-types-09` | `process.cwd()` on a `NodeJS.Process` parameter | external `global` | library `node` | C-6: a declared root is never `global`; `NodeJS` types come from `@types/node` |
| `poc-schema2-native-06` | `import … from "./broken.js"` (a file that fails to parse) | external `broken.ts` | `not_indexed` | A program file that never parsed is `not_indexed` ([TypeScript](../../../docs/languages/typescript.md)) |
| `binder-this` | `literal.run()` | `unknown_receiver` | resolved | C-2: a constant object literal's method |
| `c2-objects` | `service.start()`, `service.stop()` (in both files), `all.service.stop()` | `unknown_receiver` | resolved | C-2 |
| `c2-objects` | `service.arrow()`, `written.run()` | `unknown_receiver` | `mutable_binding` | C-2's mutation census: the member is written (or deleted) somewhere |
| `c6-node-process` | `process.cwd()` on a `NodeJS.Process` parameter | external `global` | library `node` | C-6 |
| `c8-dynamic-import` | `printReport()`, `renamed()`, `fromExpression()`, `express()` | `not_concrete` | resolved (external `express`) | C-8: a binding destructured from a literal awaited dynamic import reads that module's export |
| `builtins-members` | A call through a member of a library value: `s[0].toLowerCase()`, `e.message.trim()`, `Math.PI.toFixed()`, `Function.prototype.toString.call(f)`, `s.trim.call(s)`, `console.log.bind(console)`, `Intl.Collator.supportedLocalesOf(…)`, `globalThis.console.log(…)`, `.compare` on `new Intl.Collator(…)`, and `(Error as any).captureStackTrace(e)` | external `global` | `unknown_receiver` | A member's own value isn't read; its owner's label wouldn't say where the method is declared (the checker finds these in `ecmascript` and `global`, and the last nowhere) |

Two shapes come from the node model rather than from evidence: every dynamic `import()` is a call
node, whose `calls` link (written at its `import` keyword) is `unknown dynamic`; and in
`poc-native-binding-17` the default `d = { method() { f(); } }` names its method `outer.method`,
while the default `e = class { method() { f(); } }` is an inline class, `outer/lambda[3]`, whose
method is `outer/lambda[3].method`.
