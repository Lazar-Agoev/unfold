/** Where the repository, the built app, and the test data live. */

import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");

/** The Electron app directory: `electron <desktopDir>` runs the built `out/` with `ui/dist`. */
export const desktopDir = join(repoRoot, "desktop");

export const playgroundsDir = join(repoRoot, "testdata", "playgrounds");

export const fixturesDir = join(repoRoot, "testdata", "fixtures");

/** The Electron binary the desktop package depends on (the `electron` module exports its path). */
export function electronExecutable(): string {
  const require = createRequire(join(desktopDir, "package.json"));
  const path: unknown = require("electron");
  if (typeof path !== "string") {
    throw new Error("the electron package did not export its binary path");
  }
  return path;
}
