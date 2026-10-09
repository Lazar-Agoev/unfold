/** Running `cargo xtask` from a journey (headless exports to compare the app's graph with). */

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { repoRoot } from "./paths";

/** Cargo from `CARGO`, rustup's default location, or `PATH`. */
function cargo(): string {
  const fromEnv = process.env["CARGO"];
  if (fromEnv) {
    return fromEnv;
  }
  const rustup = join(homedir(), ".cargo", "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
  return existsSync(rustup) ? rustup : "cargo";
}

/** Runs `cargo xtask <args>` in the repository and returns its standard output. */
export function xtask(args: readonly string[]): string {
  return execFileSync(cargo(), ["xtask", ...args], {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 512,
    stdio: ["ignore", "pipe", "pipe"],
  });
}
