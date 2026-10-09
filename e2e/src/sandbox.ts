/**
 * A test's private corner of the disk: a user-data folder, a home folder (so logs and fallback
 * caches stay out of the real ones), and copies of the playgrounds. Everything lives under one
 * temporary folder outside the repository (inside it, the repository's `.gitignore` would apply to
 * the copies).
 */

import { chmodSync, cpSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { playgroundsDir } from "./paths";

export type PlaygroundName = "ts-orders" | "py-orders";

export class Sandbox {
  /** The canonical path of the temporary folder (macOS's temp folder is behind a symlink). */
  readonly root: string;
  readonly userData: string;
  readonly home: string;

  private constructor(root: string) {
    this.root = root;
    this.userData = join(root, "user-data");
    this.home = join(root, "home");
    mkdirSync(this.userData, { recursive: true });
    mkdirSync(this.home, { recursive: true });
  }

  static create(): Sandbox {
    return new Sandbox(realpathSync(mkdtempSync(join(tmpdir(), "unfold-e2e-"))));
  }

  /** A fresh copy of a playground (without any machine-local `.unfold/local/`), as `name`. */
  playground(source: PlaygroundName, name: string = source): string {
    const folder = join(this.root, name);
    cpSync(join(playgroundsDir, source), folder, { recursive: true });
    rmSync(join(folder, ".unfold", "local"), { recursive: true, force: true });
    return folder;
  }

  /** An empty folder to fill, as `name`. */
  folder(name: string): string {
    const folder = join(this.root, name);
    mkdirSync(folder, { recursive: true });
    return folder;
  }

  /** Makes a folder tree read-only (or writable again), so Unfold must use its fallback location. */
  setReadOnly(folder: string, readOnly: boolean): void {
    const walk = (path: string): void => {
      const stat = statSync(path);
      if (stat.isDirectory()) {
        if (!readOnly) {
          chmodSync(path, 0o755);
        }
        for (const entry of readdirSync(path)) {
          walk(join(path, entry));
        }
        if (readOnly) {
          chmodSync(path, 0o555);
        }
      } else {
        chmodSync(path, readOnly ? 0o444 : 0o644);
      }
    };
    walk(folder);
  }

  /** Removes everything; read-only folders are made writable first. */
  dispose(): void {
    try {
      this.setReadOnly(this.root, false);
    } catch {
      // Best effort: a file may have gone already.
    }
    rmSync(this.root, { recursive: true, force: true });
  }
}

/** Writes a file, creating its parent folders. */
export function writeFile(path: string, text: string): void {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
}

export function readFile(path: string): string {
  return readFileSync(path, "utf8");
}

/** Replaces the one occurrence of `from` in a file, failing loudly if it isn't there exactly once. */
export function editFile(path: string, from: string, to: string): void {
  const text = readFile(path);
  const first = text.indexOf(from);
  if (first < 0 || text.includes(from, first + 1)) {
    throw new Error(`expected exactly one ${JSON.stringify(from)} in ${path}`);
  }
  writeFileSync(path, text.slice(0, first) + to + text.slice(first + from.length));
}
