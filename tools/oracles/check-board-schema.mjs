// Validates board files against the unfold-board JSON Schema with an independent validator
// (Ajv, draft 2020-12), so the schema and Rust's strict parser cannot drift apart unnoticed.
//
// Usage: node tools/oracles/check-board-schema.mjs [board.json ...]
//
// With no arguments it validates testdata/boards/*.json. It also checks that the schema's two
// copies (crates/unfold-reader/schema/ and docs/schemas/) are identical. Exits non-zero on any
// failure.

import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const schemaPath = join(repository, "crates/unfold-reader/schema/unfold-board.schema.json");
const mirrorPath = join(repository, "docs/schemas/unfold-board.schema.json");

const schemaText = readFileSync(schemaPath, "utf8");
let failed = readFileSync(mirrorPath, "utf8") !== schemaText;
if (failed) console.error(`${mirrorPath} differs from ${schemaPath}`);

const ajv = new Ajv2020({ allErrors: true, strict: true });
const validate = ajv.compile(JSON.parse(schemaText));
const files = process.argv.length > 2
  ? process.argv.slice(2).map(file => resolve(file))
  : readdirSync(join(repository, "testdata/boards")).filter(name => name.endsWith(".json")).sort().map(name => join(repository, "testdata/boards", name));
for (const file of files) {
  if (validate(JSON.parse(readFileSync(file, "utf8")))) {
    console.warn(`${file}: valid`);
  } else {
    failed = true;
    console.error(`${file}:\n${validate.errors.map(error => `  ${error.instancePath || "/"} ${error.message}`).join("\n")}`);
  }
}
process.exitCode = failed ? 1 : 0;
