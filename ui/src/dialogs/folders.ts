/** Board folder paths (contract §1.4): checking one, and joining a folder name onto its parent. */

const CONTROL = /\p{Cc}/u;

/** Why a folder path can't be used, or null: 1–8 segments of 1–64 characters, no `\`, `.`, or `..`. */
export function folderProblem(path: string): string | null {
  const segments = path.split("/");
  if (segments.length > 8) {
    return "A folder can be at most 8 levels deep.";
  }
  for (const segment of segments) {
    if (segment.length === 0) {
      return "Name every folder in the path.";
    }
    if (segment.length > 64) {
      return "A folder's name can be at most 64 characters.";
    }
    if (segment !== segment.trim()) {
      return "A folder's name can't start or end with a space.";
    }
    if (segment === "." || segment === ".." || segment.includes("\\") || CONTROL.test(segment)) {
      return `“${segment}” can't be a folder's name.`;
    }
  }
  return null;
}

/** A folder's name joined onto its parent (null: the top level). */
export function childPath(parent: string | null, name: string): string {
  return parent === null ? name : `${parent}/${name}`;
}

/** The folder a path sits in (null: the top level), and its own name. */
export function splitPath(path: string): { readonly parent: string | null; readonly name: string } {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? { parent: null, name: path } : { parent: path.slice(0, slash), name: path.slice(slash + 1) };
}
