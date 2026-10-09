/**
 * A synthetic TypeScript repository of many small files that import and call each other, in two
 * "branches": switching writes every file at once with other bodies and other import targets, so
 * both per-file and cross-file facts change (a branch switch).
 */

import { join } from "node:path";
import { writeFile } from "./sandbox";

const PER_FOLDER = 25;

function filePath(index: number): string {
  return `src/m${String(Math.floor(index / PER_FOLDER)).padStart(3, "0")}/f${String(index).padStart(4, "0")}.ts`;
}

function relativeImport(from: number, to: number): string {
  const sameFolder = Math.floor(from / PER_FOLDER) === Math.floor(to / PER_FOLDER);
  const name = filePath(to).split("/").slice(-1)[0] ?? "";
  return sameFolder ? `./${name}` : `../${filePath(to).split("/").slice(-2).join("/")}`;
}

/** The text of file `index` of `count` on `branch`. */
export function syntheticFile(index: number, count: number, branch: "main" | "feature"): string {
  const step = branch === "main" ? 1 : 7;
  const callee = (index + step) % count;
  const lines = [
    `import { f${callee} } from "${relativeImport(index, callee)}";`,
    "",
    `export function f${index}(x: number): number {`,
    branch === "main" ? `  const y = x + ${index};` : `  const y = x * ${index + 2} - 1;`,
    `  if (y > ${count * 10}) {`,
    "    return y;",
    "  }",
    `  return f${callee}(y);`,
    "}",
    "",
    `export class C${index} {`,
    "  run(): number {",
    `    return f${index}(${branch === "main" ? 1 : 2});`,
    "  }",
  ];
  if (branch === "feature") {
    lines.push("", "  twice(): number {", "    return this.run() * 2;", "  }");
  }
  lines.push("}", "");
  return lines.join("\n");
}

/** Writes all `count` files of `branch` under `folder` (as fast as the disk allows). */
export function writeSyntheticRepo(folder: string, count: number, branch: "main" | "feature"): void {
  for (let index = 0; index < count; index++) {
    writeFile(join(folder, filePath(index)), syntheticFile(index, count, branch));
  }
}

export function syntheticPath(index: number): string {
  return filePath(index);
}
