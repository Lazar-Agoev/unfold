/**
 * The title bar (architecture §11.1): the folder switcher with recent folders, the board's title
 * (filled by the board view), the Code/Flow lens, Compile (quiet while the graph is current,
 * progress while compiling, cancellable), Publish and the board menu (filled by the board view),
 * and the inspector toggle. It doubles as the window's drag region.
 */

import { ChevronDown, Code2, PanelRight, Workflow, X } from "lucide-react";
import { Button, IconButton, MenuButton, SegmentedControl, Tooltip, type MenuEntry } from "../components";
import type { View } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { windowCommand } from "../ipc/unfold";
import { ActionIds, actions, useShortcut } from "../keyboard";
import { openPreferences } from "../preferences/PreferencesDialog";
import { openFolderSettings } from "../settings";
import { useCompileState } from "../state/compile";
import { useLayout } from "../state/layout";
import { setLens, useLens } from "../state/lens";
import { folderName } from "../state/window";
import { chooseFolder, openRecentFolder } from "../welcome/openFolder";
import { ShellSlot } from "./slots";
import { progressText } from "./status";
import "./TitleBar.css";
import type { CompileAction } from "./useCompile";

interface TitleBarProps {
  readonly folder: string | null;
  readonly compile: CompileAction;
}

const LENSES = [
  { value: "code", label: "Code", icon: Code2 },
  { value: "flow", label: "Flow", icon: Workflow },
] as const;

function FolderSwitcher({ folder }: { readonly folder: string }) {
  const recent = useEngineQuery("recent_folders", {});
  const others = (recent.data?.folders ?? []).filter((entry) => entry.path !== folder).slice(0, 8);
  const entries: MenuEntry[] = [
    ...(others.length > 0
      ? [
          { kind: "heading" as const, id: "recent", label: "Recent" },
          ...others.map(
            (entry): MenuEntry => ({
              id: `recent:${entry.path}`,
              label: entry.name,
              onSelect: () => void openRecentFolder(entry.path),
            }),
          ),
          { kind: "separator" as const, id: "s1" },
        ]
      : []),
    { id: "open", label: "Open Folder…", shortcut: "Mod+O", onSelect: () => void actions.run(ActionIds.openFolder) },
    { id: "open-new", label: "Open Folder in New Window…", onSelect: () => void chooseFolder("new_window") },
    { id: "new-window", label: "New Window", shortcut: "Mod+Shift+N", onSelect: () => void windowCommand("new_window", { folder: "same" }) },
    { kind: "separator", id: "s2" },
    { id: "folder-settings", label: "Folder Settings…", onSelect: openFolderSettings },
    { id: "close-folder", label: "Close Folder", onSelect: () => void actions.run(ActionIds.closeFolder) },
    { id: "preferences", label: "Preferences…", shortcut: "Mod+,", onSelect: openPreferences },
  ];
  return (
    <MenuButton
      variant="text"
      label={folderName(folder)}
      menuLabel="Folders"
      title={folder}
      className="titlebar-folder"
      icon={ChevronDown}
      entries={entries}
    />
  );
}

function CompileButton({ compile }: { readonly compile: CompileAction }) {
  const running = useCompileState((state) => state.running);
  const progress = useCompileState((state) => state.progress);
  const shortcut = useShortcut(ActionIds.compile) ?? undefined;
  const kind = compile.status?.state.kind;
  const compiling = running !== null || kind === "compiling";

  if (compiling) {
    const live = progressText(progress);
    return (
      <div className="titlebar-compiling" role="status">
        <Button size="small" variant="quiet" busy className="titlebar-compile" disabled>
          {live ? `Compiling · ${live}` : "Compiling…"}
        </Button>
        <IconButton icon={X} label="Cancel Compile" size="small" onClick={() => void compile.cancel()} />
      </div>
    );
  }
  // Quiet while the graph is current; a plain button when a Compile would change something.
  const quiet = kind === "current" || kind === "updating";
  return (
    <Tooltip content={kind === "current" ? "The graph is current. Compile checks every file again." : "Compile the folder"} {...(shortcut ? { shortcut } : {})}>
      <Button
        className="titlebar-compile"
        size="small"
        variant={quiet ? "quiet" : "secondary"}
        disabled={!compile.canCompile}
        onClick={() => {
          actions.run(ActionIds.compile);
        }}
      >
        Compile
      </Button>
    </Tooltip>
  );
}

export function TitleBar({ folder, compile }: TitleBarProps) {
  const inspectorOpen = useLayout((state) => state.inspectorOpen);
  const lens = useLens();
  const inspectorShortcut = useShortcut(ActionIds.toggleInspector) ?? undefined;
  const workbench = folder !== null && compile.status !== null && compile.status.state.kind !== "not_compiled";

  return (
    <header className="titlebar">
      {folder === null ? <span className="titlebar-app">Unfold</span> : <FolderSwitcher folder={folder} />}
      {workbench ? <ShellSlot name="titlebar.board" className="titlebar-board" /> : null}
      <div className="titlebar-spacer" />
      {workbench ? (
        <SegmentedControl<View>
          label="Lens"
          size="small"
          className="titlebar-lens"
          options={LENSES}
          value={lens}
          onChange={setLens}
        />
      ) : null}
      <div className="titlebar-spacer" />
      <div className="titlebar-actions">
        {workbench ? <CompileButton compile={compile} /> : null}
        {workbench ? <ShellSlot name="titlebar.actions" className="titlebar-slot" /> : null}
        {workbench ? (
          <IconButton
            icon={PanelRight}
            label="Toggle Inspector"
            {...(inspectorShortcut ? { shortcut: inspectorShortcut } : {})}
            pressed={inspectorOpen}
            onClick={() => {
              actions.run(ActionIds.toggleInspector);
            }}
          />
        ) : null}
        {workbench ? <ShellSlot name="titlebar.menu" className="titlebar-slot" /> : null}
      </div>
    </header>
  );
}
