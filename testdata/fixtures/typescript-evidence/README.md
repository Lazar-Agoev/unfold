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
- **The other folders** cover binder rules (`binder-*`), the override census (`overrides`), overloads, narrowing,
  receivers, script globals across files (`scripts`), the C decisions (`c2-objects`, `c4-this-parameter`,
  `c6-node-process`, `c8-dynamic-import`), written return types (`return-types`: a class or nothing, a generic type by
  its class, `await` on `async` callables, and call initializers), initializers of reassigned variables (`reassigned`)
  and overwritten class properties (`class-fields`), which aren't followed, and ECMAScript's standard objects
  (`builtins`: its globals by name, and a built-in method on a literal or a written class type, beside host globals,
  which stay `library global`; `builtins-overrides`: a written class type a project class may override is
  `overridden`, and structural interfaces give no receiver; `builtins-inherited`: a method a project class inherits
  from an ECMAScript base is a builtin, and a member inherited from outside the project is `overridden` when a
  subclass declares it; `builtins-patched`: a project write to a global or a prototype makes its calls `dynamic`,
  inherited ones included; `builtins-instances`: so does a write to an instance's member, for every value of its type;
  `builtins-replaced`: so is every member of what `new` of a replaced global constructs; `builtins-members`: `Intl`'s
  values and globals read from `globalThis` are builtins, and a call through a member of a library value is
  `unknown_receiver`, while the member itself keeps its label). In `class-fields` the checker resolves the `save`
  calls of `Swapped`, `Constructed`, `Exposed`, `Hidden`, and `Base` to `Repo.save` by declared type, where Unfold,
  which can't prove the property's value, leaves them unknown.

The goldens' calls resolved to a project definition name the TypeScript checker's target, except C-8 in
`c8-dynamic-import`: Unfold reads the module's export for `printReport()`, `renamed()`, and `fromExpression()`, which
the checker leaves unresolved. No test compares them; `tools/oracles/typescript-oracle.mjs` can, on a copy of a
folder.

Two shapes come from the node model rather than from evidence: every dynamic `import()` is a call
node, whose `calls` link (written at its `import` keyword) is `unknown dynamic`; and in
`poc-native-binding-17` the default `d = { method() { f(); } }` names its method `outer.method`,
while the default `e = class { method() { f(); } }` is an inline class, `outer/lambda[3]`, whose
method is `outer/lambda[3].method`.
