/**
 * Which paths `open_folder` may open: only folders in the engine's recent list (architecture §13:
 * paths outside a folder come only from dialogs, the recent list, or a drop). Pure, so it is tested
 * without Electron.
 */

/**
 * Whether a `recent_folders` result lists `folder`. Both sides are compared after `canonical`
 * (the main process passes `realpath`), so a symlinked or differently spelled path still matches.
 */
export function isRecentFolder(recent: unknown, folder: string, canonical: (path: string) => string): boolean {
  if (!isRecord(recent) || !Array.isArray(recent["folders"])) {
    return false;
  }
  const wanted = canonical(folder);
  return recent["folders"].some(
    (item: unknown) => isRecord(item) && typeof item["path"] === "string" && canonical(item["path"]) === wanted,
  );
}

/** `open_folder`'s target: `target` ("this_window" | "new_window") or `newWindow: true`; null if malformed. */
export function openFolderTarget(args: Readonly<Record<string, unknown>>): "this_window" | "new_window" | null {
  const { target, newWindow } = args;
  if (newWindow !== undefined && typeof newWindow !== "boolean") {
    return null;
  }
  if (target === undefined) {
    return newWindow === true ? "new_window" : "this_window";
  }
  if (target !== "this_window" && target !== "new_window") {
    return null;
  }
  return newWindow === undefined || (newWindow === (target === "new_window")) ? target : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
