# Test data

Inputs and expected outputs for Unfold's tests. Crates keep no test data of their own. Which test reads or writes
each file, and how to bless goldens, is in [docs/testing.md](../docs/testing.md#goldens).

```text
testdata/
├── playgrounds/
│   ├── unfold.toml              # the mixed folder: both playgrounds opened together
│   ├── ts-orders/               # TypeScript: Express routes, BullMQ workers, node-cron, a CLI
│   └── py-orders/               # Python: FastAPI routes, Celery tasks, APScheduler jobs, console scripts
├── fixtures/                    # small exact cases
│   ├── entrances/               # two apps, two entrances (oracle corpus `entrances`)
│   ├── python/policy/           # binder positives and refusals, source roots, encodings
│   ├── python-lowering/         # the worked examples, as Python modules
│   ├── python-ranges/           # Unicode and CRLF ranges (oracle corpus `python-ranges`)
│   ├── typed-lines/             # a TypeScript and a Python orders flow with written types
│   ├── typescript-evidence/     # binding and evidence cases, each with expected links (its README)
│   ├── typescript-structure/    # one source per syntax area, each with its node tree
│   └── typescript-typed/        # typed-record cases, each source with its `.typed` snapshot
├── goldens/
│   ├── <playground>.jsonl       # canonical exports: ts-orders, py-orders, mixed-playground, typed-lines
│   ├── <playground>.entrances.txt  # the state of every `code` key in the `[[app]]` tables
│   ├── <playground>.flows.txt   # every definition drawn, as agent text: ts-orders, py-orders, mixed-playground, typed-lines
│   ├── drawn/*.json             # drawn flows as the IPC sends them
│   └── sample.jsonl             # unfold-core's sample `shop` graph
├── workspace/commands/          # command results on ts-orders (the UI tests' engine answers), and in
│                                #   py-orders/ and mixed-playground/ a smaller set on those playgrounds
├── boards/                      # two board files on ts-orders
├── reference/oracles/           # cached oracle answers, floors, justified entries (reference/README.md)
└── corpora.lock.toml            # larger corpora, pinned by hash (and by URL where public)
```

## Conventions

- **Never compile in place.** Tests copy a folder to a temporary directory before compiling, so nothing writes
  `.unfold/local/` into `testdata/`. Only blessing (`UNFOLD_BLESS=1`) writes here.
- **Only shared files are committed:** sources, `unfold.toml`, and board files. Never `.unfold/local/`.
- **Bytes are exact.** Hashes, board signatures, and offsets depend on them, and some fixtures are CRLF or Latin-1
  on purpose. `.gitattributes` turns off line-ending conversion. Don't reformat anything.
- **Nothing is installed or run.** `package.json`, `pyproject.toml`, and scripts are analyzed inputs. There is no
  `node_modules`, `.venv`, build output, or lockfile.
- Paths in expectations are relative to the folder opened, with `/`.

## Playgrounds

| Folder | Sources | `[[app]]` tables |
| --- | --- | --- |
| `ts-orders` | 34 TypeScript files under `src/`, plus `package.json` and `tsconfig.json` | 7 apps, 18 entrances |
| `py-orders` | 50 Python files under `src/`; `legacy/`, `scripts/`, and `tests/` are outside `[scope]` | 7 apps, 18 entrances |
| `playgrounds/` itself | Both, with paths prefixed by folder | 14 apps, 36 entrances |

Open a playground folder itself. The mixed folder's `unfold.toml` repeats both playgrounds' settings with prefixed
paths; the nested `unfold.toml` files aren't read then.

Each `unfold.toml` gives every process the package starts an app (`run`, `code`), with the routes, queues,
schedules, and commands it handles as entrances. The tables cover every entrance state:

- **found**: almost every key;
- **missing, with one fix**: "Admin stats" names a definition that moved to `routes/admin`, and the comment above it
  says so;
- **not analyzed**: the JavaScript in `ts-orders` (`report-processor.js`, `scripts/export.js`) and the legacy export
  in `py-orders` (`legacy/` is a source root but out of scope).

The `.entrances.txt` goldens record these states. Their format is documented at the top of
`crates/unfold-lang-typescript/tests/structure/goldens.rs`.

## Fixtures

- **`entrances/`**: a small Express server and a queue worker, declared as a Server and a Worker app with one
  entrance each.
- **`python/policy/`**: Python binder cases, one behavior per file (arguments, bindings, classes, decorators,
  dynamic code, evaluation order, headers, imports, receivers). `single-root/` and `duplicate-roots/` each have their own `unfold.toml` and are
  opened on their own. `encoding/latin1_cookie.py` is Latin-1 on purpose and must fail as a file.
- **`python-lowering/`**: the worked examples (`shop`, `inbox`, `mail`, `pipeline`, `booking`), with modules that
  define what they call. The Python lowering tests check their trees, and the census runs over them.
- **`python-ranges/`**: `fixtures/unicode.py`, with astral and Greek identifiers and CRLF line ends. Its source root
  is `fixtures`.
- **`typed-lines/`**: one project with `ts/` (NodeNext, `stripe` named as a dependency, nothing installed) and
  `py/` (a Python project with source root `src`). Their calls carry written types: parameters of every shape,
  generics, interfaces and aliases, dataclasses, `TypedDict`, pydantic models, enums, raised errors, and heritage
  chains. `expected/typescript.jsonl` and `expected/python.jsonl` snapshot the typed records.
- **`typescript-evidence/`**: see [its README](fixtures/typescript-evidence/README.md).
- **`typescript-structure/`**: `ambient`, `calls`, `classes`, `destructuring`, `imports`, `names`, `statements`, and
  `types`, each a `.ts` source and the `.tree` it lowers to (every field of every node).
- **`typescript-typed/`**: `heritage`, `members`, `results`, `signatures`, and `type-names`, each a `.ts` source and
  the `.typed` snapshot of its typed records.

## Boards

Both boards target `playgrounds/ts-orders` opened on its own, with real keys and real signatures.

- **`order-placement.json`**, "Placing an order": the entrance "Place an order" and five definitions, with names and
  combined steps. The reader loads it healthy against the `ts-orders` golden graph, checks it round-trips byte for
  byte, and checks the overlay rules on save. Command and Home tests open it on a compiled copy.
- **`order-system.json`**, "How an order is placed": nested frames, every resource kind (a table backed by
  `interface Order`), a connection of each of the seven kinds, notes, and a proposed step. The reader checks it
  round-trips and repairs without leaving anything dangling; the workspace saves it to itself as its own golden.

Both follow the [board schema](../docs/schemas/unfold-board.schema.json);
`node tools/oracles/check-board-schema.mjs` validates them.

## Oracles and corpora

[`reference/README.md`](reference/README.md) describes the precision oracles. [`corpora.lock.toml`](corpora.lock.toml)
pins the larger corpora by the hash of their selected files, with a URL where the source is public. Among them are
TypeScript 6.0.3's `src`, the proof of concept's own source, and the FastAPI full-stack template 0.12.0; the lock
file lists them all.
