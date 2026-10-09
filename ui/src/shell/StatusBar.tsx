/**
 * The status bar (architecture §7.9, §11.5): the graph's state, quiet notes (fallback location,
 * watching by scanning), the problem count, the engine connection when it isn't connected, and
 * the canvas's zoom readout. Clicking the state opens Compile, Rebuild from scratch, Pause or
 * Resume background updates, Show index log, and Copy Diagnostics.
 */

import { useQueryClient } from "@tanstack/react-query";
import { TriangleAlert } from "lucide-react";
import { useRef, useState } from "react";
import { ContextMenu, showToast, type MenuEntry } from "../components";
import { call } from "../ipc/commands";
import { useConnectionState } from "../ipc/hooks";
import { engineKey, useEngineQuery } from "../ipc/queries";
import { toUnfoldError, type ConnectionState } from "../ipc/unfold";
import { useCompileState } from "../state/compile";
import { copyDiagnostics } from "../settings";
import { useLayout } from "../state/layout";
import { ShellSlot } from "./slots";
import { plural, statusLine } from "./status";
import "./StatusBar.css";
import type { CompileAction } from "./useCompile";

const CONNECTION_LABELS: Readonly<Record<ConnectionState, string | null>> = {
  connecting: "Connecting to the engine…",
  connected: null,
  reconnecting: "Reconnecting…",
  unavailable: "Engine unavailable",
};

function GraphStatusItem({ compile }: { readonly compile: CompileAction }) {
  const client = useQueryClient();
  const progress = useCompileState((state) => state.progress);
  const button = useRef<HTMLButtonElement>(null);
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const status = compile.status;
  if (!status) {
    return null;
  }
  const line = statusLine(status, progress);

  const setBackground = async (on: boolean) => {
    try {
      const next = await call("set_background_updates", { on });
      client.setQueryData(engineKey("graph_status", {}), next);
    } catch (error) {
      showToast({ message: `Couldn't change background updates: ${toUnfoldError(error).message}`, tone: "danger" });
    }
  };

  const entries: MenuEntry[] = [
    { id: "compile", label: "Compile", shortcut: "Mod+Shift+B", disabled: !compile.canCompile, onSelect: () => void compile.compile("check").catch(() => undefined) },
    {
      id: "rebuild",
      label: "Rebuild from scratch",
      disabled: !compile.canCompile,
      onSelect: () => void compile.compile("from_scratch").catch(() => undefined),
    },
    ...(compile.isCompiling ? [{ id: "cancel", label: "Cancel Compile", onSelect: () => void compile.cancel() }] : []),
    { kind: "separator", id: "s1" },
    status.backgroundUpdates
      ? { id: "pause", label: "Pause background updates", onSelect: () => void setBackground(false) }
      : { id: "resume", label: "Resume background updates", onSelect: () => void setBackground(true) },
    {
      id: "log",
      label: "Show index log",
      onSelect: () => {
        useLayout.getState().showPanelTab("index_log");
      },
    },
    { kind: "separator", id: "s2" },
    { id: "diagnostics", label: "Copy Diagnostics", onSelect: () => void copyDiagnostics() },
  ];

  return (
    <>
      <button
        ref={button}
        type="button"
        className="statusbar-item statusbar-button"
        data-tone={line.tone}
        aria-haspopup="menu"
        aria-expanded={menuAt !== null}
        onClick={() => {
          const rect = button.current?.getBoundingClientRect();
          setMenuAt(rect ? { x: rect.left, y: rect.top - 4 } : { x: 0, y: 0 });
        }}
      >
        <span className="statusbar-glyph" aria-hidden="true">
          {line.glyph}
        </span>
        <span role="status">{line.text}</span>
      </button>
      {line.notes.map((note) => (
        <span key={note.text} className="statusbar-item statusbar-note" title={note.title}>
          {note.text}
        </span>
      ))}
      {menuAt ? (
        <ContextMenu
          at={menuAt}
          label="Graph"
          entries={entries}
          onClose={() => {
            setMenuAt(null);
          }}
        />
      ) : null}
    </>
  );
}

function ProblemCount() {
  const problems = useEngineQuery("problems", {});
  const count = problems.data?.length ?? 0;
  if (count === 0) {
    return null;
  }
  return (
    <button
      type="button"
      className="statusbar-item statusbar-button statusbar-problems"
      aria-label={`${plural(count, "problem")}, show Problems`}
      onClick={() => {
        useLayout.getState().showPanelTab("problems");
      }}
    >
      <TriangleAlert size={12} aria-hidden="true" />
      {count.toLocaleString("en-US")}
    </button>
  );
}

export function StatusBar({ compile, folder }: { readonly compile: CompileAction; readonly folder: string | null }) {
  const connection = useConnectionState();
  const connectionLabel = CONNECTION_LABELS[connection];
  return (
    <footer className="statusbar">
      {folder !== null && connection === "connected" ? <GraphStatusItem compile={compile} /> : null}
      {connectionLabel ? (
        <span className="statusbar-item" role="status" data-connection={connection}>
          <span className="statusbar-dot" aria-hidden="true" />
          {connectionLabel}
        </span>
      ) : null}
      <span className="statusbar-spacer" />
      {folder !== null && connection === "connected" ? <ProblemCount /> : null}
      <ShellSlot name="statusbar.zoom" className="statusbar-slot" />
    </footer>
  );
}
