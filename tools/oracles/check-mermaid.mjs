// Parses Unfold's Mermaid output with the real Mermaid parser (in Node, over jsdom), so a
// snapshot that renders wrong in Mermaid fails here instead of in someone's README.
//
// Usage: node tools/oracles/check-mermaid.mjs [file ...]
//
// Each file is either a `.mmd` diagram or a JSON object whose string values (at any depth) are
// diagrams, like the snapshots in testdata/mermaid/. With no arguments it checks every Unfold
// snapshot: testdata/boards/*.mmd and testdata/mermaid/*.json. Exits non-zero if any diagram
// fails to parse.

import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { JSDOM } from "jsdom";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function defaultFiles() {
  const inDirectory = (directory, extension) =>
    readdirSync(join(repository, directory))
      .filter(name => name.endsWith(extension))
      .sort()
      .map(name => join(repository, directory, name));
  return [...inDirectory("testdata/boards", ".mmd"), ...inDirectory("testdata/mermaid", ".json")];
}

/** Every diagram in a file, as [label, text] pairs. */
function diagrams(path) {
  const text = readFileSync(path, "utf8");
  if (!path.endsWith(".json")) return [[path, text]];
  const found = [];
  const walk = (value, label) => {
    if (typeof value === "string") found.push([label, value]);
    else if (value && typeof value === "object") for (const [key, item] of Object.entries(value)) walk(item, `${label} › ${key}`);
  };
  walk(JSON.parse(text), path);
  return found;
}

async function main() {
  const files = process.argv.length > 2 ? process.argv.slice(2).map(file => resolve(file)) : defaultFiles();
  // Mermaid sanitizes labels with DOMPurify, which needs a window.
  const dom = new JSDOM("<!doctype html><html><body></body></html>");
  Object.defineProperty(globalThis, "window", { configurable: true, value: dom.window });
  Object.defineProperty(globalThis, "document", { configurable: true, value: dom.window.document });
  const mermaid = (await import("mermaid")).default;
  mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
  let checked = 0;
  const failures = [];
  for (const file of files) {
    for (const [label, text] of diagrams(file)) {
      checked++;
      try {
        await mermaid.parse(text);
      } catch (error) {
        failures.push(`${label}\n    ${String(error instanceof Error ? error.message : error).split("\n").join("\n    ")}`);
      }
    }
  }
  dom.window.close();
  if (failures.length) {
    console.error(`${failures.length} of ${checked} diagrams failed to parse:\n${failures.join("\n")}`);
    process.exitCode = 1;
  } else {
    console.warn(`${checked} diagrams parse with mermaid ${mermaid.version ?? ""}`.trim());
  }
}

await main();
