/**
 * Choose folders… on the first-Compile screen: narrow the scope to some top-level folders before
 * the first Compile, which helps with large monorepos. It writes `[scope] include` through
 * `update_folder_config` (Graph only this folder, then Include in graph for the rest).
 */

import { useState } from "react";
import { Button, Dialog, showToast } from "../components";
import { call } from "../ipc/commands";
import type { DirEntry, SettingsEdit } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { toUnfoldError } from "../ipc/unfold";
import "./FirstCompile.css";

/** The edits that graph exactly `chosen` (none when every folder is chosen and no scope is set). */
export function scopeEdits(chosen: readonly string[], all: readonly string[], include: readonly string[]): SettingsEdit[] {
  if (chosen.length === all.length && include.length === 0) {
    return [];
  }
  const [first, ...rest] = chosen;
  if (first === undefined) {
    return [];
  }
  return [{ op: "graph_only", path: first }, ...rest.map((path): SettingsEdit => ({ op: "include_in_graph", path }))];
}

function selectable(entry: DirEntry): boolean {
  return entry.kind === "folder" && !entry.unfold && (entry.skipped === null || entry.skipped === "not_included");
}

export function ChooseFoldersDialog({ onClose }: { readonly onClose: () => void }) {
  const listing = useEngineQuery("list_dir", { path: "" });
  const config = useEngineQuery("get_folder_config", {});
  const folders = (listing.data?.entries ?? []).filter(selectable);
  const include = config.data?.settings?.include ?? [];
  const [chosen, setChosen] = useState<ReadonlySet<string> | null>(null);
  const [saving, setSaving] = useState(false);

  const initial = new Set(folders.filter((entry) => include.length === 0 || entry.skipped === null).map((entry) => entry.path));
  const current = chosen ?? initial;

  const toggle = (path: string, on: boolean) => {
    const next = new Set(current);
    if (on) {
      next.add(path);
    } else {
      next.delete(path);
    }
    setChosen(next);
  };

  const apply = async () => {
    const edits = scopeEdits(
      folders.map((entry) => entry.path).filter((path) => current.has(path)),
      folders.map((entry) => entry.path),
      include,
    );
    if (edits.length === 0) {
      onClose();
      return;
    }
    setSaving(true);
    try {
      await call("update_folder_config", { edits });
      onClose();
    } catch (error) {
      showToast({ message: `Couldn't change the scope: ${toUnfoldError(error).message}`, tone: "danger" });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog
      open
      onClose={onClose}
      title="Choose folders to graph"
      description="Only the checked folders are compiled. This is saved in .unfold/config.toml, so you can widen it later."
      width="medium"
      footer={
        <>
          <Button variant="quiet" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" busy={saving} disabled={current.size === 0 || saving} onClick={() => void apply()}>
            Use these folders
          </Button>
        </>
      }
    >
      {listing.error ? <p className="first-compile-error">{listing.error.message}</p> : null}
      {folders.length === 0 && listing.data ? <p className="first-compile-note">This folder has no folders to choose from.</p> : null}
      <ul className="choose-folders">
        {folders.map((entry) => (
          <li key={entry.path}>
            <label className="choose-folder">
              <input
                type="checkbox"
                checked={current.has(entry.path)}
                onChange={(event) => {
                  toggle(entry.path, event.target.checked);
                }}
              />
              <span>{entry.name}</span>
            </label>
          </li>
        ))}
      </ul>
    </Dialog>
  );
}
