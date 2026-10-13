/** Board folder names (docs/reader.md#boards): folders are one level deep, so a folder is one name. */

const CONTROL = /\p{Cc}/u;

/** Why a folder name can't be used, or null: 1–64 characters, no `/` or `\`, not `.` or `..`. */
export function folderProblem(name: string): string | null {
  if (name.length === 0) {
    return "Name the folder.";
  }
  if (name.length > 64) {
    return "A folder's name can be at most 64 characters.";
  }
  if (name !== name.trim()) {
    return "A folder's name can't start or end with a space.";
  }
  if (name.includes("/")) {
    return "A folder's name can't contain “/”: folders hold boards, not other folders.";
  }
  if (name === "." || name === ".." || name.includes("\\") || CONTROL.test(name)) {
    return `“${name}” can't be a folder's name.`;
  }
  return null;
}
