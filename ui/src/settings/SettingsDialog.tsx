/**
 * Settings (app-wide, `settings.json` through Rust; the sidebar's gear): the editor command, and how to
 * connect an agent. The editor command saves with `set_settings` when the field is left, on Enter, and
 * when the dialog closes.
 */

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { Dialog, TextField, showToast } from "../components";
import { call } from "../ipc/commands";
import type { AppSettings } from "../ipc/generated";
import { engineKey, engineQueryOptions } from "../ipc/queries";
import { toUnfoldError } from "../ipc/unfold";
import { AgentsSection } from "./AgentsSection";
import "./settings.css";

function SettingsForm({ saved, onClose }: { readonly saved: AppSettings; readonly onClose: () => void }) {
  const client = useQueryClient();
  const [editor, setEditor] = useState(saved.editorCommand ?? "");
  // What was last saved, so leaving the field and then closing writes once, and a failed save is tried again.
  const stored = useRef(saved);

  const commit = async () => {
    const editorCommand = editor.trim();
    if (editorCommand === (stored.current.editorCommand ?? "")) {
      return;
    }
    const settings: AppSettings = editorCommand === "" ? {} : { editorCommand };
    try {
      const next = await call("set_settings", { settings });
      stored.current = next;
      client.setQueryData(engineKey("get_settings", {}), next);
    } catch (error) {
      showToast({ title: "Couldn't save the editor command", body: toUnfoldError(error).message, tone: "danger" });
    }
  };

  const close = () => {
    void commit();
    onClose();
  };

  return (
    <Dialog open onClose={close} title="Settings" width={470} placement="window" className="app-settings">
      <section className="app-settings-section" aria-labelledby="app-settings-editor">
        <h3 id="app-settings-editor" className="app-settings-caps">
          Editor
        </h3>
        <TextField
          label="Editor command"
          placeholder="code -g {path}:{line}"
          className="app-settings-editor"
          value={editor}
          onChange={(event) => {
            setEditor(event.target.value);
          }}
          onBlur={() => void commit()}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              void commit();
            }
          }}
          hint="{path}, {line}, and {column} are replaced. Empty opens files in TextEdit."
        />
      </section>
      <AgentsSection />
    </Dialog>
  );
}

/** The open dialog. It reads the settings each time it opens, so it never edits a stale copy. */
export function SettingsDialog({ onClose }: { readonly onClose: () => void }) {
  const settings = useQuery({ ...engineQueryOptions("get_settings", {}), refetchOnMount: "always" });
  if (!settings.isFetchedAfterMount || !settings.data) {
    return null;
  }
  return <SettingsForm saved={settings.data} onClose={onClose} />;
}
