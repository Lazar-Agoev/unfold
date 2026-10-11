/**
 * Locators as people type them in the manual entrance editor (docs/engine.md, "Locator
 * grammar"): a syntax check that mirrors Rust's `Locator::parse` so mistakes show while typing, and
 * the locator a definition gets when it is added as an entrance. Rust stays the judge: it validates
 * again when the edit is applied and resolves locators on every compile.
 */

/** The forms a locator can take, for the editor's inline help. */
export const LOCATOR_FORMS: readonly { readonly form: string; readonly selects: string }[] = [
  { form: "path", selects: "the file itself (a startup)" },
  { form: "path#name", selects: "an exported or top-level name" },
  { form: "path#Owner.member", selects: "a method of a class or object" },
  { form: "path#default", selects: "the default export" },
  { form: "path:line", selects: "the outermost function, method, or branch on that line" },
];

const NAME = /^[\p{ID_Start}$_][\p{ID_Continue}$‌‍]*$/u;
const LINE = /^[1-9][0-9]*$/;
const MAX_LINE = Number.MAX_SAFE_INTEGER;

/** Why a folder-relative path can't be a locator's path, or null. */
function pathProblem(path: string): string | null {
  if (path === "") {
    return "Start with a folder-relative file path, such as src/server.ts.";
  }
  if (path.includes("\\")) {
    return "Use / between folders, not \\.";
  }
  if (path.includes("\0")) {
    return "The path can't contain a NUL character.";
  }
  if (path.startsWith("/")) {
    return "Use a path relative to the folder, not an absolute one.";
  }
  if (path.split("/").some((part) => part === "" || part === "." || part === "..")) {
    return "The path can't have empty, “.”, or “..” parts.";
  }
  return null;
}

/** Why `text` isn't a well-formed locator, or null when it is. */
export function locatorProblem(text: string): string | null {
  // A drive prefix reads as a line selector otherwise.
  if (/^[A-Za-z]:/.test(text)) {
    return "Use a path relative to the folder, not an absolute one.";
  }
  const selectors = text.split(/[#:]/).length - 1;
  if (selectors > 1) {
    return "A locator has at most one # or : (a path can't contain either).";
  }
  const hash = text.indexOf("#");
  const colon = text.indexOf(":");
  const cut = hash >= 0 ? hash : colon;
  const path = cut >= 0 ? text.slice(0, cut) : text;
  const problem = pathProblem(path);
  if (problem) {
    return problem;
  }
  if (hash >= 0) {
    const selector = text.slice(hash + 1);
    const names = selector.split(".");
    if (selector === "" || names.length > 2 || !names.every((name) => NAME.test(name))) {
      return "After # comes a name (createOrder), Owner.member (OrderService.place), or default.";
    }
  }
  if (colon >= 0) {
    const line = text.slice(colon + 1);
    if (!LINE.test(line) || Number(line) > MAX_LINE) {
      return "After : comes a line number, such as 12.";
    }
  }
  return null;
}

/** The language a locator's file belongs to, from its extension (null when it's neither). */
export function locatorLanguage(text: string): "typescript" | "python" | null {
  const cut = text.search(/[#:]/);
  const path = cut >= 0 ? text.slice(0, cut) : text;
  if (path.endsWith(".py")) {
    return "python";
  }
  if (/\.(ts|tsx|mts|cts)$/.test(path)) {
    return "typescript";
  }
  return null;
}

/** What the editor needs to know about a definition to point a locator at it. */
export interface DefinitionRef {
  readonly path: string;
  /** The block kind, such as `function` or `method`. */
  readonly kind: string;
  readonly name: string;
  readonly qualifiedName: string;
  readonly line: number | null;
}

const NAMED_KINDS: ReadonlySet<string> = new Set(["function", "class", "variable"]);
const MEMBER_KINDS: ReadonlySet<string> = new Set(["method", "getter", "setter", "constructor"]);
const HANDLER_KINDS: ReadonlySet<string> = new Set([...NAMED_KINDS, ...MEMBER_KINDS]);

/** Whether "Add as Entrance…" makes sense for a definition of this kind. */
export function canBeHandler(kind: string): boolean {
  return HANDLER_KINDS.has(kind);
}

/**
 * The locator for a definition: `path#name` for a top-level declaration, `path#Owner.member` for a
 * member, and `path:line` for anything nested deeper (which a name can't select).
 */
export function locatorFor(definition: DefinitionRef): string {
  const parts = definition.qualifiedName.split(".");
  const named =
    (NAMED_KINDS.has(definition.kind) && parts.length === 1) || (MEMBER_KINDS.has(definition.kind) && parts.length === 2);
  if (named && locatorProblem(`${definition.path}#${definition.qualifiedName}`) === null) {
    return `${definition.path}#${definition.qualifiedName}`;
  }
  return definition.line === null ? definition.path : `${definition.path}:${definition.line}`;
}
