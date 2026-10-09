/**
 * The settings actions: the palette's Add Entrance…, Folder Settings…, and Copy Diagnostics (also
 * the Help menu's), and "Add as Entrance…" from a definition's context menu.
 */

import { ActionIds, useRegisterActions, type ActionSpec } from "../keyboard";
import { useWindow } from "../state/window";
import { copyDiagnostics } from "./diagnostics";
import { canBeHandler, locatorFor, type DefinitionRef } from "./locators";
import { openEntranceEditor, openFolderSettings } from "./store";

const hasFolder = () => typeof useWindow.getState().info?.folder === "string";

/** Opens the entrance editor with the definition as the first handler. */
export function addAsEntrance(definition: DefinitionRef): void {
  openEntranceEditor({ mode: "add", handler: locatorFor(definition) });
}

/** The "Add as Entrance…" menu item for a definition, or none when it can't be a handler. */
export function addAsEntranceItem(definition: DefinitionRef): { id: string; label: string; onSelect: () => void }[] {
  return canBeHandler(definition.kind)
    ? [
        {
          id: "add-entrance",
          label: "Add as Entrance…",
          onSelect: () => {
            addAsEntrance(definition);
          },
        },
      ]
    : [];
}

export function useSettingsActions(): void {
  const specs: ActionSpec[] = [
    {
      id: "settings.addEntrance",
      title: "Add Entrance…",
      category: "Settings",
      keywords: ["manual", "entrance", "config.toml"],
      enabled: hasFolder,
      run: () => {
        openEntranceEditor();
      },
    },
    {
      id: "settings.folderSettings",
      title: "Folder Settings…",
      category: "Settings",
      keywords: ["python", "detectors", "scope", "config.toml"],
      enabled: hasFolder,
      run: openFolderSettings,
    },
    {
      id: ActionIds.copyDiagnostics,
      title: "Copy Diagnostics",
      category: "Help",
      keywords: ["bug", "report", "version"],
      run: copyDiagnostics,
    },
  ];
  useRegisterActions(specs);
}
