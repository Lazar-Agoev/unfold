// Exports the reader reference: what the proof of concept's `reader/core` derives from its frozen
// mixed playground generation, per playground, with paths and keys mapped onto Unfold's testdata
// folders exactly as `testdata/reference/corpora.toml` maps them.
//
// Usage: node tools/oracles/export-reader.mjs --poc <graph-compiler checkout> [--out <dir>]
//
// The PoC is only read: its TypeScript runs in memory through the PoC's own installed `tsx`, and
// nothing is written inside it. See tools/oracles/README.md.

import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** The playgrounds to export: the Unfold folder name and the corpus whose path mapping applies. */
const PLAYGROUNDS = [
  { name: "ts-orders", corpus: "playground-ts-native", board: "order-placement" },
  { name: "py-orders", corpus: "python-playground" },
];

/** The PoC reader functions the export calls, loaded in `main`. */
let pocReader;

function parseArguments(argv) {
  const options = { poc: process.env.UNFOLD_POC, out: join(repository, "testdata/reference/poc-reader") };
  for (let index = 0; index < argv.length; index++) {
    const flag = argv[index];
    const value = argv[index + 1];
    if ((flag === "--poc" || flag === "--out") && value !== undefined) {
      options[flag.slice(2)] = resolve(value);
      index++;
    } else {
      throw new Error(`unknown argument ${flag}\nusage: export-reader.mjs --poc <graph-compiler> [--out <dir>]`);
    }
  }
  if (!options.poc) throw new Error("pass --poc <graph-compiler checkout> (or set UNFOLD_POC)");
  return options;
}

/** The `paths` mapping of one `[[corpus]]` table in corpora.toml (a single inline-table array). */
function corpusPaths(corporaToml, corpus) {
  const tables = corporaToml.split(/^\[\[corpus\]\]$/m).slice(1);
  const table = tables.find(text => new RegExp(`^name = "${corpus}"$`, "m").test(text));
  if (!table) throw new Error(`corpora.toml has no corpus ${corpus}`);
  const paths = /^paths = \[([\s\S]*?)\]$/m.exec(table);
  if (!paths) throw new Error(`corpus ${corpus} has no paths`);
  const mappings = [...paths[1].matchAll(/\{\s*poc = "([^"]*)",\s*unfold = "([^"]*)"\s*\}/g)].map(match => ({ poc: match[1], unfold: match[2] }));
  if (mappings.length === 0) throw new Error(`corpus ${corpus} has an empty path mapping`);
  // The longest matching prefix wins.
  return mappings.sort((a, b) => b.poc.length - a.poc.length);
}

/** Maps a PoC path, or `undefined` when it lies outside the corpus. */
function mapPath(mappings, path) {
  const mapping = mappings.find(item => path.startsWith(item.poc));
  return mapping && mapping.unfold + path.slice(mapping.poc.length);
}

/**
 * Entrance keys of the corpus's canonical reference, in order, grouped by the PoC's key for them without
 * the `#n` ordinal: `entrance:` and the JSON array of detector, kind, method, trigger, and first target
 * text. (Unfold keys a manual entrance by its configured ID instead.)
 */
function canonicalEntranceKeys(canonicalPath) {
  const entrances = [];
  const first = new Map();
  for (const line of readFileSync(canonicalPath, "utf8").split("\n")) {
    if (line.startsWith('{"t":"entrance"')) entrances.push(JSON.parse(line));
    if (line.startsWith('{"t":"target"')) {
      const target = JSON.parse(line);
      if (target.position === 0) first.set(target.entrance, target.text);
    }
  }
  const groups = new Map();
  for (const entrance of entrances) {
    const base = `entrance:${JSON.stringify([entrance.detector, entrance.kind, entrance.method ?? "", entrance.trigger ?? "", first.get(entrance.key) ?? ""])}`;
    groups.set(base, [...(groups.get(base) ?? []), entrance.key]);
  }
  return groups;
}

const byText = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const sortedKeys = values => [...new Set(values)].sort(byText);

function exportPlayground(repo, playground, mappings, canonicalKeys) {
  const generation = repo.generation;
  const blockKey = id => {
    const key = repo.keyForId(id);
    const mapped = mapPath(mappings, key);
    if (mapped === undefined) throw new Error(`block key ${key} is outside ${playground.name}`);
    return mapped;
  };
  const inPlayground = block => mapPath(mappings, block.file_path) !== undefined;

  // Entrances of this playground: evidence under the playground, or a root settings entrance whose
  // targets point into it (the TypeScript manual entrances live in the root codegraph.json).
  const targetsOf = new Map();
  for (const target of generation.entrypointTargets) targetsOf.set(target.entrypoint_id, [...(targetsOf.get(target.entrypoint_id) ?? []), target]);
  const mine = generation.entrypoints.filter(entry => {
    if (mapPath(mappings, entry.evidence_file_path) !== undefined && entry.evidence_file_path !== "codegraph.json") return true;
    return (targetsOf.get(entry.id) ?? []).some(target => {
      const block = repo.blocks.get(target.target_id);
      return block ? inPlayground(block) : target.target_text.startsWith(mappings[0].poc) && mappings[0].poc !== "";
    });
  });
  const entranceKeys = new Map();
  const used = new Map();
  for (const entry of mine) {
    const targets = targetsOf.get(entry.id) ?? [];
    let first = targets[0]?.target_text ?? "";
    if (entry.detector === "manual") first = mapPath(mappings, first) ?? first;
    const base = `entrance:${JSON.stringify([entry.detector, entry.kind, entry.method, entry.trigger, first])}`;
    const candidates = canonicalKeys.get(base) ?? [];
    const ordinal = used.get(base) ?? 0;
    used.set(base, ordinal + 1);
    if (ordinal >= candidates.length) throw new Error(`no canonical entrance for ${base} (#${ordinal})`);
    entranceKeys.set(repo.keyForId(entry.id), candidates[ordinal]);
  }
  const canonicalCount = [...canonicalKeys.values()].reduce((total, keys) => total + keys.length, 0);
  if (entranceKeys.size !== canonicalCount) throw new Error(`${playground.name}: ${entranceKeys.size} entrances, canonical has ${canonicalCount}`);
  const entranceKey = key => {
    const mapped = entranceKeys.get(key);
    if (!mapped) throw new Error(`entrance ${key} is outside ${playground.name}`);
    return mapped;
  };

  for (const path of Object.keys(generation.manifest.files).filter(path => mapPath(mappings, path) !== undefined)) {
    const status = repo.verifiedSource(path).status;
    if (status !== "current") throw new Error(`${path} is ${status} in the PoC checkout; the reference needs current sources`);
  }

  const definitions = repo.definitions()
    .filter(item => mapPath(mappings, item.filePath) !== undefined)
    .map(item => ({ pocKey: item.key, key: mapPath(mappings, item.key), kind: item.kind, name: item.name, qualifiedName: item.qualifiedName }))
    .sort((a, b) => byText(a.key, b.key));
  const catalog = definitions.map(({ pocKey: _, ...item }) => item);

  const overview = repo.overview();
  const applications = overview.applications
    .filter(application => mapPath(mappings, application.filePath) !== undefined)
    .map(application => ({
      startup: mapPath(mappings, application.startupKey),
      // The PoC sidebar's application name: the file name without its extension, then `#` and the
      // console callable's qualified name.
      name: application.filePath.split("/").at(-1).replace(/\.(?:[cm]?[jt]sx?|py)$/, "") + (application.startupName ? `#${application.startupName}` : ""),
      launches: sortedKeys(application.launches.map(launch => entranceKey(launch.key))),
      entrances: sortedKeys(application.entrances.map(entry => entranceKey(entry.key))),
    }))
    .sort((a, b) => byText(a.startup, b.startup));
  const ownedEntrance = entry => entranceKeys.has(entry.key);
  const overviewOut = {
    applications,
    otherEntrances: sortedKeys(overview.otherEntrances.filter(ownedEntrance).map(entry => entranceKey(entry.key))),
    unindexedLaunches: sortedKeys(overview.unindexedLaunches.filter(ownedEntrance).map(entry => entranceKey(entry.key))),
  };

  const optional = value => (value === "" || value === undefined ? null : value);
  const relations = {};
  for (const item of definitions) {
    const found = repo.relations(item.pocKey);
    relations[item.key] = {
      callers: found.callers
        .map(caller => ({ owner: mapPath(mappings, caller.definition.key), source: mapPath(mappings, caller.call.sourceKey), kind: caller.call.kind, position: optional(caller.call.position) }))
        .sort((a, b) => byText(JSON.stringify(a), JSON.stringify(b))),
      outgoing: found.callees.map(call => ({
        source: mapPath(mappings, call.sourceKey), kind: call.kind, target: call.targetKey === null ? null : mapPath(mappings, call.targetKey),
        resolution: call.resolution, reason: optional(call.reason), module: optional(call.module), position: optional(call.position),
        text: call.targetKey === null ? call.targetName : null,
      })),
      reachedFrom: sortedKeys(found.reachedFrom.map(entry => entranceKey(entry.key))),
    };
  }

  const closures = {};
  for (const entry of mine) {
    const targets = (targetsOf.get(entry.id) ?? []).filter(target => target.resolution === "resolved").map(target => repo.blocks.get(target.target_id));
    let seeds = targets;
    if (entry.kind === "process") {
      seeds = targets.slice(0, 1);
      const startup = seeds[0];
      if (startup && entry.detector === "pyproject.script") {
        let module = startup;
        while (module.parent_id) module = repo.blocks.get(module.parent_id);
        if (module.id !== startup.id) seeds.push(module);
      }
    }
    closures[entranceKey(repo.keyForId(entry.id))] = sortedKeys([...repo.closure(seeds, true)].map(blockKey));
  }
  const sortedClosures = Object.fromEntries(Object.keys(closures).sort(byText).map(key => [key, closures[key]]));

  // Mermaid for every entrance (`codegraph export --entrance <key> --mermaid`), with the
  // corpus's paths mapped in the text.
  const mermaid = {};
  for (const entry of [...mine].sort((a, b) => byText(entranceKey(repo.keyForId(a.id)), entranceKey(repo.keyForId(b.id))))) {
    mermaid[entranceKey(repo.keyForId(entry.id))] = mapText(mappings, pocReader.exportEntrance(repo, repo.keyForId(entry.id)));
  }

  // The Flow model of every function-like definition and class, compacted: exact source
  // (`code`) is dropped, chips become their occurrence key and edge kind.
  const mapKey = key => (key === null ? null : mapPath(mappings, key) ?? key);
  const chip = item => ({ source: mapKey(item.sourceKey), kind: item.kind, target: mapKey(item.targetKey), name: item.targetName });
  const mapItems = items => items.map(item => {
    if (item.type === "node") return item;
    if (item.type === "branch") return { ...item, yes: mapItems(item.yes), no: mapItems(item.no) };
    if (item.type === "loop") return { ...item, body: mapItems(item.body) };
    return { ...item, key: mapKey(item.key), catchKey: mapKey(item.catchKey), body: mapItems(item.body), handler: mapItems(item.handler) };
  });
  const flows = {};
  for (const item of definitions) {
    const flow = pocReader.buildFlow(repo, item.pocKey);
    flows[item.key] = {
      nodes: flow.nodes.map(node => ({
        id: node.id, key: mapKey(node.key), kind: node.kind, label: node.label, lane: node.lane,
        ...(node.described ? { described: true } : {}),
        ...(node.checks ? { checks: node.checks, outcome: node.outcome } : {}),
        ...(node.call ? { call: chip(node.call) } : {}),
        chips: node.chips.map(chip),
        ...(node.callbacks ? { callbacks: node.callbacks.map(callback => ({ key: mapKey(callback.key), lines: callback.lines, exits: callback.exits })) } : {}),
      })),
      edges: flow.edges.map(edge => [edge.from, edge.to, edge.label, ...(edge.dashed ? [true] : [])]),
      tree: mapItems(flow.tree), start: flow.start, end: flow.end, lanes: flow.lanes, warnings: flow.warnings,
    };
  }
  return { catalog, applications: overviewOut, relations, closures: sortedClosures, mermaid, flows };
}

/** Maps every PoC path prefix of the corpus inside free text (Mermaid labels and comments). */
function mapText(mappings, text) {
  let result = text;
  for (const mapping of mappings) if (mapping.poc !== "") result = result.split(mapping.poc).join(mapping.unfold);
  return result;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  // Run the PoC's TypeScript in memory with its own tsx; nothing is compiled into its tree.
  process.env.TSX_DISABLE_CACHE = "1";
  const tsx = await import(pathToFileURL(join(options.poc, "reader/node_modules/tsx/dist/esm/api/index.mjs")).href);
  tsx.register();
  const { Repository } = await import(pathToFileURL(join(options.poc, "reader/core/repository.ts")).href);
  const { buildFlow } = await import(pathToFileURL(join(options.poc, "reader/core/flow.ts")).href);
  const { exportEntrance, exportMermaid } = await import(pathToFileURL(join(options.poc, "reader/core/mermaid.ts")).href);
  const { loadBoard } = await import(pathToFileURL(join(options.poc, "reader/core/boards.ts")).href);
  pocReader = { buildFlow, exportEntrance };
  const repo = new Repository(options.poc);
  const corporaToml = readFileSync(join(repository, "testdata/reference/corpora.toml"), "utf8");
  for (const playground of PLAYGROUNDS) {
    const mappings = corpusPaths(corporaToml, playground.corpus);
    const canonicalKeys = canonicalEntranceKeys(join(repository, "testdata/reference/canonical", `${playground.corpus}.jsonl`));
    const exported = exportPlayground(repo, playground, mappings, canonicalKeys);
    const directory = join(options.out, playground.name);
    mkdirSync(directory, { recursive: true });
    for (const [name, value] of Object.entries(exported)) writeFileSync(join(directory, `${name}.json`), `${JSON.stringify(value, null, 2)}\n`);
    if (playground.board) {
      // `codegraph export --board order-placement --mermaid`: the sample board the Unfold copy
      // in testdata/boards was converted from.
      const { board } = loadBoard(repo, playground.board);
      writeFileSync(join(directory, `board-${playground.board}.mmd`), mapText(mappings, exportMermaid(repo, board)));
    }
    const counts = `${exported.catalog.length} definitions, ${exported.applications.applications.length} applications, ${Object.keys(exported.closures).length} closures, ${Object.keys(exported.mermaid).length} entrance diagrams, ${Object.keys(exported.flows).length} flows`;
    console.warn(`${playground.name}: ${counts}`);
  }
}

await main();
