/**
 * The application menu runs the same actions as the keyboard: the main process forwards a clicked
 * menu item as a `menu_command` event, and this runs the action it maps to.
 */

import { useEffect } from "react";
import { onEvent, type MenuCommand } from "../ipc/unfold";
import { MENU_COMMAND_ACTIONS } from "./keymap";
import { actions as windowActions, type ActionRegistry } from "./registry";

function isMenuCommand(value: unknown): value is MenuCommand {
  return typeof value === "string" && Object.hasOwn(MENU_COMMAND_ACTIONS, value);
}

/** Runs the action for a menu command. Returns whether it ran (unknown or disabled commands don't). */
export function runMenuCommand(command: unknown, registry: ActionRegistry = windowActions): boolean {
  return isMenuCommand(command) && registry.run(MENU_COMMAND_ACTIONS[command]);
}

/** Routes the window's `menu_command` events to actions while mounted. */
export function useMenuCommands(registry: ActionRegistry = windowActions): void {
  useEffect(
    () =>
      onEvent("menu_command", (event) => {
        runMenuCommand(event["command"], registry);
      }),
    [registry],
  );
}
