# Graph model

Read this for graph semantics, identity, coordinates, validation, or canonical output.
Exact record fields are owned by [model.rs](../crates/unfold-core/src/model.rs) and
[typed.rs](../crates/unfold-core/src/typed.rs); vocabularies and permitted combinations by
[vocab.rs](../crates/unfold-core/src/vocab.rs). Do not maintain a second field or enum list here.

## Records

| Record | Meaning |
| --- | --- |
| File | Indexed source path, language, facts state, hash, and parse/read failure |
| Input | Fingerprinted configuration or evidence file; never also a source file |
| Block | Selected syntax entity, parent, kind/role, source range, local ID, structural key |
| Slot | Written part of a block, optionally repeated with a contiguous index |
| Edge | Relationship at a written occurrence, with target or external/unresolved outcome |
| Entrance | Declared process, command, HTTP route, worker, schedule, or event, with evidence |
| Entrance target | Ordered startup or handler target of an entrance |
| Typed records | Written signatures, parameters, members, arguments, call results, type links |

Edges originate at occurrences (`call`, `new`, `import`), not at functions. Function-level
owners, callers, closures, applications, and relations are [reader projections](reader.md#derived-views).
Entrance handlers remain in declaration order, including middleware; this is not execution
order. Every entrance has targets; a process has exactly one startup target.

A file is `current`, `last_good`, or `failed`. Current and last-good files carry facts;
a failed file carries no graph or typed records. Last-good facts refer to their own hash,
not to the current disk bytes. Keep failure and completeness visible to callers.

## Typed records

Only written types are recorded. There is no inferred type or transitive exception analysis.
Ranges are token-exact bytes; short written text is capped at 120 UTF-16 units.

| Record | Cardinality / rule |
| --- | --- |
| Signature | One per function-like block; an optional written return annotation |
| Parameter | One per parameter slot, same index; language kind, name, flags, written annotation |
| Member | Source-ordered written fields/signatures of a type declaration or class |
| Argument | One per argument slot; expansion, keyword, proven pairing, optional bound count |
| Call result | One per call/new; where its value lands, whether awaited, proven receiver binding |
| Type link | A written name in recorded annotations or heritage, resolved by the shared policy |

For a resolved occurrence, each argument has exactly one proven parameter or unpaired
reason; unresolved/external occurrences have neither. A passed callable's bound count
belongs to the value being passed, not to the receiving call's arguments.

Result binding is `variable`, `return`, `argument`, `discard`, or `other`. Parentheses and
`await` are transparent; TypeScript also treats assertions, `!`, and `satisfies` as transparent.
Only an exact initializer, returned expression, ordinary argument, or expression statement
gets the corresponding binding. Record an argument's outer call and slot position explicitly.

Type links resolve only to same-language class/interface/type-alias/enum declarations.
Heritage records distinguish `extends` from `implements`; generic arguments are ordinary
links. Dynamic bases do not become static heritage. Language-specific exclusions and string
annotations are in [TypeScript](languages/typescript.md#typed-records) and [Python](languages/python.md#typed-records).

## Text, ranges, and lines

- Snapshots preserve bytes, BOM, line endings, and normalization. Decode strictly as UTF-8;
  files exceeding `u32::MAX` bytes fail. Python rejects declared non-UTF-8 encodings.
- Offsets are zero-based UTF-8 bytes; ranges are half-open `[start, end)` on character boundaries.
  A root spans the whole file. Other blocks and slots exclude leading comments/indentation
  and trailing whitespace, but retain formatting inside their token-exact range.
- Lines are one-based. CR, LF, CRLF, U+2028, and U+2029 end lines; CRLF counts once.
  For nonempty ranges, the end line contains `end - 1`. Empty files span `[0, 0)` on line 1.
- Rendered offsets are UTF-16. Convert through core's text utilities; offsets inside a UTF-8
  sequence or between surrogate halves are errors, never rounded.
- Truncate labels without splitting surrogate pairs. Python identity uses NFKC while display
  text and ranges retain the written spelling.

Implementation: [text/](../crates/unfold-core/src/text/mod.rs).

## Paths

Project paths are nonempty, relative, case-preserving, `/`-separated UTF-8. Reject empty,
`.` or `..` components, backslashes, NUL, drive prefixes, and absolute paths. Order by UTF-8
bytes. Runtime access must additionally enforce symlink confinement; syntactic path validity
alone does not establish it. See [RepoPath](../crates/unfold-core/src/path.rs) and
[workspace confinement](../crates/unfold-workspace/src/confine.rs).

## Identity and structural keys

Local IDs are preorder positions within a file. SQL addresses a block with `(file_id, local_id)`;
resolved cross-file targets, boards, and canonical export use structural keys.

| Key part | Rule |
| --- | --- |
| Root | `path#` |
| Named child | `kind:name`, with `[n]` after the first sibling of that kind/name |
| Unnamed child | `kind[n]`, counting earlier siblings of its kind, named or unnamed |
| Nested child | Parent key plus `/step`; root children follow `#` directly |
| Entrance | `entrance:` plus the canonical JSON identity tuple, with duplicate ordinal suffix |
| Configured entrance | `entrance:configured:` plus its `[[entrance]]` table's `id` |

Names escape `%`, `/`, `#`, `[`, and `]` using uppercase percent encoding. Keys are derived,
never chosen: validation re-derives them. The exception is a manual entrance, whose key is the
`id` its `[[entrance]]` table declares, so it survives edits to the table's other values. SQL interns keys for the database's lifetime.
Ordinal steps can shift after insertions; [board guards](reader.md#guards-and-reconnecting)
prevent silently attaching an explanation to different code. Renames/moves require reconnecting.

Exact derivation, entrance identity, and duplicate ordering: [key.rs](../crates/unfold-core/src/key.rs).

## Order and canonical export

Source files and inputs sort by path; blocks are preorder; repeated parts retain source order.
Slots, edges, typed records, entrances, and targets have canonical record-specific ordering.
Preserve these rules when changing extraction, concurrency, updates, or serialization.

Canonical export is a deterministic, versioned JSONL interchange, independent of SQL IDs.
References use keys; a header identifies the format and producer versions. The canonical
reader/writer own exact field names, omission rules, record ordering, and compatibility:
[canonical/](../crates/unfold-core/src/canonical/mod.rs).

Use `cargo xtask canonical-check <export.jsonl>` for round-tripping, `canonical-diff` for
record comparisons, and parity/incremental tests for behavior changes. Do not hand-edit
frozen exports merely to make a test pass.

## Validation invariants

Every compile validates before committing; failure keeps the previous graph. Validation
checks structural consistency, not correct runtime dispatch or complete reachability.
The complete checks live in [validate/](../crates/unfold-core/src/validate/mod.rs).

- Files/inputs are uniquely ordered, mutually exclusive by path, and consistent with their
  hashes, failures, and states. Block keys are globally unique.
- Block 0 is the only file root; `blocks[i].local_id == i`. Other blocks name an open ancestor
  as parent. Parent ranges/lines contain children; siblings follow source order.
- Kinds, roles, and slots are permitted for the language. Repeated slot indices start at zero
  and are contiguous; singleton slots have no index. Slot ranges are nonempty and contained.
- Edge sources are permitted occurrence kinds. Sequence 0 is the occurrence edge; references
  follow in argument order. Targets, reasons, modules, and positions fit the outcome/context.
- A resolved target exists and has a permitted kind. External results carry a module;
  unresolved results carry a reason. Neither supplies a pretend concrete target.
- Typed record cardinalities match slots/blocks. Parameter/member indices are contiguous;
  ranges belong to their written annotations. Pairings agree with resolution and parameter bounds.
- Entrance evidence belongs to a recorded source/input and its exact declaration range or
  configuration pointer. Target roles, positions, kinds, and keys are consistent.
- Language census checks ensure every required selected occurrence has been represented;
  never improve recall by silently dropping difficult occurrences.

## Value policy and reason order

Languages provide binding, receiver, declaration, and import evidence; the shared backend
selects outcomes. Candidate order must not change the answer. A declaration must identify
one concrete permitted target before an edge can resolve.

The ordered policy is implemented in [resolve/policy.rs](../crates/unfold-engine/src/resolve/policy.rs):

1. Language refusal, then ambiguous evidence, then unbound `this`.
2. Root import failures, then any effective unindexed repository candidate.
3. External package origins and absence of concrete candidates.
4. Mixed origins, unnamed declarations, or multiple distinct concrete targets.
5. No concrete target: prefer parameter, mutable binding, accessor, then not-concrete reasons.
6. Incompatible non-concrete declarations beside a body; dispatch/override uncertainty;
   unsupported target kinds; otherwise resolve to the unique target.

Compatible overload signatures beside one implementation do not create extra targets.
The override census includes descendants through declared bases and unknown computed names;
static, private, and `super` accesses follow their specific exemptions. Preserve precedence
by changing the shared policy rather than choosing a reason independently in a front end.

Module labels retain useful external identity: package/module/global/builtin forms, not
repository paths exposed as external packages. Language-specific module rules live in each
language guide; label serialization lives in core.

## Provenance

Framework recognition requires proven package identity, followed through supported stable
imports, aliases, members, annotations, and factories. A matching local name is insufficient.
Reflections, writes, wrappers, uncertain factories, and hidden registration can invalidate
body proof while an entrance remains listed. Keep detection separate from handler resolution.
See [detectors](engine.md#entrance-detectors) and [provenance.rs](../crates/unfold-engine/src/provenance.rs).

## Pairing

Pair arguments only for the resolved occurrence, after proving the number of bound leading
parameters. The shared [pairing.rs](../crates/unfold-engine/src/pairing.rs) applies these rules:

- Drop proven bound parameters and explicit `this` parameters before assigning arguments.
  Unknown binding leaves arguments unpaired with `receiver`.
- Positional arguments fill positional takers in order, then rest; excess is `no_parameter`.
- A spread and later positional arguments are `spread`. Python `**` is itself spread;
  later explicit keywords may still pair by name.
- Python keywords fill an eligible named taker, then `**kwargs`; a duplicate positional/keyword
  assignment is `no_parameter`. Equal ordinals alone never prove a binding.
- A class with no direct constructor takes no arguments only when the language proves it;
  otherwise arguments are `no_signature`. Defaults do not fill missing parameters.
- Reference edges are not paired. Record a passed callable's own bound count separately.
