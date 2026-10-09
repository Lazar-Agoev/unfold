// The TypeScript precision oracle: asks the published TypeScript checker (the exact version pinned
// in package.json) what every call, construction, and named argument of a corpus targets, and
// writes one JSONL file that `cargo xtask oracles check` compares with Unfold's graph.
//
// Usage: node tools/oracles/typescript-oracle.mjs --folder <corpus> --export <canonical.jsonl>
//          --corpus <name> --out <oracle.jsonl>
//
// `--export` is Unfold's canonical export of that folder. The oracle uses it only to name blocks:
// sites and targets are found in the checker's own syntax trees and mapped to the export's block
// with exactly that byte range. It never reads Unfold's edges. The corpus is only read.
// Format and rules: tools/oracles/README.md and testdata/reference/README.md ("Oracles").

import { createHash } from "node:crypto";
import { createReadStream, existsSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import readline from "node:readline";
import ts from "typescript";
import { CheckerQuestions, overrideCensus, unwrap } from "./typescript-oracle/checker.mjs";

const FORMAT = "unfold-oracle";
const TARGET_KINDS = new Set(["function", "method", "constructor", "getter", "setter", "class"]);

function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!["--folder", "--export", "--corpus", "--out"].includes(flag) || value === undefined) {
      throw new Error(`unknown argument ${flag}\nusage: typescript-oracle.mjs --folder <corpus> --export <canonical.jsonl> --corpus <name> --out <oracle.jsonl>`);
    }
    options[flag.slice(2)] = flag === "--corpus" ? value : resolve(value);
  }
  for (const required of ["folder", "export", "corpus", "out"]) {
    if (!options[required]) throw new Error(`--${required} is required`);
  }
  return options;
}

/** A path relative to the working directory when it lies inside it, for messages. */
function displayPath(path) {
  const inside = relative(process.cwd(), path);
  return inside.startsWith("..") ? path : inside.split(sep).join("/");
}

const byBytes = (a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b));
const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");

/** The parts of the export the oracle needs: TypeScript files, inputs, and block ranges. */
async function readExport(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  const files = [];
  const inputs = [];
  const sites = new Map();
  const targets = new Map();
  let header;
  for await (const line of readline.createInterface({ input: createReadStream(path), crlfDelay: Infinity })) {
    if (line.startsWith('{"t":"block"')) {
      const block = JSON.parse(line);
      const file = block.key.slice(0, block.key.indexOf("#"));
      const [start, end] = block.range;
      if (block.kind === "call" || block.kind === "new") sites.set(`${file}\0${start}\0${end}`, { file, key: block.key, used: false });
      else if (TARGET_KINDS.has(block.kind)) targets.set(`${file}\0${start}\0${end}\0${block.kind}`, block.key);
    } else if (line.startsWith('{"t":"file"')) {
      files.push(JSON.parse(line));
    } else if (line.startsWith('{"t":"input"')) {
      inputs.push(JSON.parse(line));
    } else if (line.startsWith('{"t":"header"')) {
      header = JSON.parse(line);
    }
  }
  if (header?.format !== "unfold-graph") throw new Error(`${path} is not an Unfold canonical export`);
  return { sha256: hash.digest("hex"), files, inputs, sites, targets };
}

/** UTF-16 index → UTF-8 byte offset for one text. */
function byteOffsets(text) {
  const offsets = new Uint32Array(text.length + 1);
  let bytes = 0;
  for (let index = 0; index < text.length; index++) {
    offsets[index] = bytes;
    const unit = text.charCodeAt(index);
    if (unit < 0x80) bytes += 1;
    else if (unit < 0x800) bytes += 2;
    else if (unit >= 0xd800 && unit < 0xdc00 && index + 1 < text.length) {
      offsets[index + 1] = bytes;
      bytes += 4;
      index++;
    } else bytes += 3;
  }
  offsets[text.length] = bytes;
  return offsets;
}

/** Reads and verifies every TypeScript source and input the export names; returns the corpus hash. */
function readCorpus(folder, graph) {
  const sources = new Map();
  const listing = [];
  for (const file of graph.files) {
    if (file.language !== "typescript") continue;
    if (file.state !== "current" || !file.hash) throw new Error(`${file.path} is ${file.state} in the export; the oracle needs a complete graph`);
    const bytes = readFileSync(join(folder, file.path));
    if (sha256(bytes) !== file.hash) throw new Error(`${file.path} does not match the export's hash: the export is not of this folder`);
    const text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
    sources.set(resolve(folder, file.path), { path: file.path, text, offsets: undefined });
    listing.push([file.path, file.hash]);
  }
  for (const input of graph.inputs) {
    if (sha256(readFileSync(join(folder, input.path))) !== input.hash) throw new Error(`input ${input.path} does not match the export's hash`);
    listing.push([input.path, input.hash]);
  }
  listing.sort((a, b) => byBytes(a[0], b[0]));
  return { sources, corpusSha256: sha256(listing.map(([path, hash]) => `${hash}  ${path}\n`).join("")) };
}

/** Configuration groups: the nearest `tsconfig.json` inside the folder, as the proof of concept grouped files. */
function configurationGroups(folder, sources) {
  const nearest = new Map();
  const find = directory => {
    if (nearest.has(directory)) return nearest.get(directory);
    const candidate = join(directory, "tsconfig.json");
    const parent = dirname(directory);
    const result = existsSync(candidate) ? candidate : directory === folder || parent === directory ? undefined : find(parent);
    nearest.set(directory, result);
    return result;
  };
  const groups = new Map();
  for (const absolute of [...sources.keys()].sort(byBytes)) {
    const config = find(dirname(absolute)) ?? "";
    if (!groups.has(config)) groups.set(config, []);
    groups.get(config).push(absolute);
  }
  return [...groups].map(([config, files]) => ({ config, files, options: compilerOptions(config) }));
}

function compilerOptions(config) {
  if (!config) {
    return { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.NodeNext, moduleResolution: ts.ModuleResolutionKind.NodeNext, noEmit: true };
  }
  const raw = ts.readConfigFile(config, ts.sys.readFile);
  if (raw.error) throw new Error(ts.flattenDiagnosticMessageText(raw.error.messageText, "\n"));
  // Indexed files are the program roots; a configuration's own file selection never applies.
  const parsed = ts.parseJsonConfigFileContent({ ...raw.config, files: [], include: [], exclude: [] }, ts.sys, dirname(config), undefined, config);
  const errors = parsed.errors.filter(error => error.code !== 18002 && error.code !== 18003);
  if (errors.length) throw new Error(`${config}: ${errors.map(error => ts.flattenDiagnosticMessageText(error.messageText, "\n")).join("\n")}`);
  return { ...parsed.options, noEmit: true };
}

const isCallSite = node => ts.isNewExpression(node) || ts.isTaggedTemplateExpression(node)
  || (ts.isCallExpression(node) && node.expression.kind !== ts.SyntaxKind.ImportKeyword);

/** A named argument: an identifier or member access once transparent wrappers are removed. */
function namedArgument(argument) {
  const value = unwrap(argument);
  if (ts.isIdentifier(value) || ts.isPropertyAccessExpression(value)) return value;
  if (ts.isElementAccessExpression(value)) {
    const key = unwrap(value.argumentExpression);
    if (ts.isStringLiteral(key) || ts.isNumericLiteral(key) || ts.isNoSubstitutionTemplateLiteral(key)) return value;
  }
  return undefined;
}

function evaluateGroup(group, sources, graph, results) {
  const host = ts.createCompilerHost(group.options, true);
  const getSourceFile = host.getSourceFile.bind(host);
  const readFile = host.readFile.bind(host);
  host.readFile = fileName => sources.get(resolve(fileName))?.text ?? readFile(fileName);
  host.getSourceFile = (fileName, languageVersion, onError, shouldCreate) => {
    const source = sources.get(resolve(fileName));
    return source ? ts.createSourceFile(fileName, source.text, languageVersion, true) : getSourceFile(fileName, languageVersion, onError, shouldCreate);
  };
  const program = ts.createProgram({ rootNames: group.files, options: group.options, host });
  const locate = node => {
    const file = node.getSourceFile();
    const source = sources.get(resolve(file.fileName));
    if (!source) return undefined;
    source.offsets ??= byteOffsets(source.text);
    return { path: source.path, start: source.offsets[node.getStart(file)], end: source.offsets[node.end] };
  };
  const blockKey = (path, start, end, kind) => graph.targets.get(`${path}\0${start}\0${end}\0${kind}`);
  const questions = new CheckerQuestions(program.getTypeChecker(), locate, blockKey);
  for (const absolute of group.files) {
    const file = program.getSourceFile(absolute);
    if (!file) throw new Error(`the checker did not load ${absolute}`);
    const pending = [file];
    while (pending.length) {
      const node = pending.pop();
      if (ts.isClassLike(node)) {
        const shape = questions.classShape(node);
        if (shape) results.shapes.push(shape);
      }
      if (isCallSite(node)) results.rows.push(...siteRows(node, questions, locate, graph));
      const children = [];
      ts.forEachChild(node, child => { children.push(child); });
      for (let index = children.length - 1; index >= 0; index--) pending.push(children[index]);
    }
  }
}

function siteRows(node, questions, locate, graph) {
  const place = locate(node);
  const site = graph.sites.get(`${place.path}\0${place.start}\0${place.end}`);
  if (site) site.used = true;
  const at = site ? { site: site.key } : { file: place.path, range: [place.start, place.end] };
  const order = [place.path, place.start, place.end];
  const callee = ts.isTaggedTemplateExpression(node) ? node.tag : node.expression;
  const context = ts.isNewExpression(node) || callee.kind === ts.SyntaxKind.SuperKeyword ? "construct" : "call";
  const rows = [{ order: [...order, -1], row: { t: "call", ...at, ...shape(questions.answer(callee, context)) } }];
  if (!site) return rows;
  (ts.isTaggedTemplateExpression(node) ? [] : node.arguments ?? []).forEach((argument, position) => {
    const value = namedArgument(argument);
    if (!value) return;
    const answer = questions.answer(value, "reference");
    if (answer.target) rows.push({ order: [...order, position], row: { t: "reference", site: site.key, position, ...shape(answer) } });
  });
  return rows;
}

/** The row fields of one answer, in a fixed order. */
function shape(answer) {
  if (!answer.target) return answer.detail ? { outcome: answer.outcome, detail: answer.detail } : { outcome: answer.outcome };
  const fields = { target: answer.target };
  if (answer.refused) fields.refused = answer.refused;
  if (answer.override) fields.override = answer.override;
  return fields;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const folder = resolve(options.folder);
  const graph = await readExport(options.export);
  const { sources, corpusSha256 } = readCorpus(folder, graph);
  const results = { rows: [], shapes: [] };
  for (const group of configurationGroups(folder, sources)) evaluateGroup(group, sources, graph, results);

  const overridden = overrideCensus(results.shapes);
  for (const { row } of results.rows) {
    if (!row.override) continue;
    if (!row.refused && overridden(row.override)) row.refused = "overridden";
    delete row.override;
  }
  results.rows.sort((a, b) => byBytes(a.order[0], b.order[0]) || a.order[1] - b.order[1] || a.order[2] - b.order[2] || a.order[3] - b.order[3]);
  const typescriptFiles = new Set([...sources.values()].map(source => source.path));
  const unmatched = [...graph.sites.values()]
    .filter(site => !site.used && typescriptFiles.has(site.file))
    .map(site => site.key)
    .sort(byBytes);
  const calls = results.rows.filter(({ row }) => row.t === "call");
  const header = {
    t: "header",
    format: FORMAT,
    version: 1,
    corpus: options.corpus,
    language: "typescript",
    oracle: "typescript",
    oracle_version: ts.version,
    corpus_sha256: corpusSha256,
    export_sha256: graph.sha256,
    files: sources.size,
    sites: calls.length,
    resolved: calls.filter(({ row }) => row.target && !row.refused).length,
    references: results.rows.length - calls.length,
    unfold_sites_without_checker_site: unmatched.length,
  };
  const text = [header, ...results.rows.map(({ row }) => row)].map(value => JSON.stringify(value)).join("\n") + "\n";
  mkdirSync(dirname(options.out), { recursive: true });
  writeFileSync(options.out, text);
  const outcomes = new Map();
  for (const { row } of calls) {
    const label = row.target ? (row.refused ? `target, refused ${row.refused}` : "target") : row.outcome;
    outcomes.set(label, (outcomes.get(label) ?? 0) + 1);
  }
  console.warn(`${displayPath(options.out)}: ${calls.length} call sites, ${header.resolved} resolved by the checker, ${header.references} named-argument targets`);
  console.warn(`  outcomes: ${[...outcomes].sort((a, b) => b[1] - a[1]).map(([label, count]) => `${label} ${count}`).join(", ")}`);
  if (unmatched.length) console.warn(`  ${unmatched.length} Unfold call/new block(s) match no checker call site, e.g. ${unmatched.slice(0, 5).join(", ")}`);
  const unmapped = calls.filter(({ row }) => row.outcome === "unmapped");
  if (unmapped.length) console.warn(`  ${unmapped.length} target(s) map to no Unfold block, e.g. ${unmapped.slice(0, 5).map(({ row }) => row.detail).join(", ")}`);
}

await main();
