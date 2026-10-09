/**
 * The activity bar: one icon per sidebar view (⌘1–⌘4); clicking the active view's icon hides the
 * sidebar. Preferences sit at the bottom.
 */

import type { LucideIcon } from "lucide-react";
import { DoorOpen, Files, LayoutGrid, Search, Settings } from "lucide-react";
import { IconButton } from "../components";
import { ActionIds, useShortcutLookup } from "../keyboard";
import { openPreferences } from "../preferences/PreferencesDialog";
import { useLayout, type SidebarView } from "../state/layout";
import "./ActivityBar.css";

export const VIEWS: readonly { view: SidebarView; label: string; icon: LucideIcon; action: string }[] = [
  { view: "files", label: "Files", icon: Files, action: ActionIds.showFiles },
  { view: "entrances", label: "Entrances", icon: DoorOpen, action: ActionIds.showEntrances },
  { view: "search", label: "Search", icon: Search, action: ActionIds.showSearch },
  { view: "boards", label: "Boards", icon: LayoutGrid, action: ActionIds.showBoards },
];

export function ActivityBar() {
  const active = useLayout((state) => (state.sidebarVisible ? state.view : null));
  const showView = useLayout((state) => state.showView);
  const shortcutOf = useShortcutLookup();

  return (
    <nav className="activitybar" aria-label="Views">
      {VIEWS.map(({ view, label, icon, action }) => {
        const shortcut = shortcutOf(action) ?? undefined;
        return (
          <IconButton
            key={view}
            icon={icon}
            label={label}
            {...(shortcut ? { shortcut } : {})}
            size="large"
            tooltipSide="right"
            className="activitybar-button"
            pressed={active === view}
            onClick={() => {
              showView(view, { toggle: true });
            }}
          />
        );
      })}
      <span className="activitybar-spacer" />
      <IconButton
        icon={Settings}
        label="Preferences"
        shortcut="Mod+,"
        size="medium"
        tooltipSide="right"
        className="activitybar-button activitybar-settings"
        onClick={openPreferences}
      />
    </nav>
  );
}
