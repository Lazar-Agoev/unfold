# TypeScript evidence cases

Each folder is a small TypeScript project (`tsconfig.json`: ESNext, NodeNext, no `types`, unless the
folder has its own) and an `expected.edges` golden: one line per edge,
`source #seq kind text -> outcome`, where the outcome is the target block, `external <module>`, or the
reason (`@n` is a parameter position). `crates/unfold-lang-typescript/tests/structure/evidence.rs`
compiles every folder and compares; `UNFOLD_BLESS=1` rewrites the goldens.

- **`poc-<suite>-<n>`**: the inline fixture sources of the proof of concept's tests
  `compilers/codegraph/test/<suite>.test.ts` (native binding, classes, declared types, narrowing,
  array and library receivers, schema 2 values, references, and native cases), in the order they appear
  there, at commit `5c98a6e`.
- **The other folders** were written for Unfold: binder rules (`binder-*`), the override census
  (`overrides`), overloads, narrowing, receivers, script globals across files (`scripts`), and the fixed
  defects (`c2-objects`, `c4-this-parameter`, `c6-node-process`, `c8-dynamic-import`).

Every golden was compared with the proof of concept's native producer (its CLI, `index --frontend
native`, run on a copy of the folder) and with its checker-backed producer. Every edge Unfold resolves
has the checker's target where the checker resolves it, and the only resolved edges the checker leaves
unresolved are the C-8 ones below. Against the native producer the goldens agree edge for edge,
except for these deliberate differences:

| Folder | Edge | Native producer | Unfold | Why |
| --- | --- | --- | --- | --- |
| `poc-native-declared-types-09` | `process.cwd()` on a `NodeJS.Process` parameter | external `global` | external `node` | C-6: a declared root is never `global`; `NodeJS` types come from `@types/node` |
| `poc-schema2-native-06` | `import … from "./broken.js"` (a file that fails to parse) | external `broken.ts` | `not_indexed` | A program file that never parsed is `not_indexed` (`docs/languages/typescript.md`, Modules) |
| `binder-this` | `literal.run()` | `unknown_receiver` | resolved | C-2: a constant object literal's method |
| `c2-objects` | `service.start()`, `service.stop()` (in both files), `all.service.stop()` | `unknown_receiver` | resolved | C-2 |
| `c2-objects` | `service.arrow()`, `written.run()` | `unknown_receiver` | `mutable_binding` | C-2's mutation census: the member is written (or deleted) somewhere |
| `c4-this-parameter` | `first()`, `second()` in `withThis(this: Window, …)` | `parameter` @1, @2 | `parameter` @0, @1 | C-4: positions skip an explicit `this` parameter |
| `c6-node-process` | `process.cwd()` on a `NodeJS.Process` parameter | external `global` | external `node` | C-6 |
| `c8-dynamic-import` | `printReport()`, `renamed()`, `fromExpression()`, `express()` | `not_concrete` | resolved (external `express`) | C-8: a binding destructured from a literal awaited dynamic import reads that module's export |
