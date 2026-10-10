/**
 * Keyboard and actions (UI plan §8): the registry every command lives in, the keymap, the person's key
 * bindings over it, the shortcut dispatcher, application-menu routing, command scopes (⌘Z to the
 * focused board), modal layers, and named focus targets.
 */

export {
  createKeyBindings,
  effectiveShortcuts,
  findConflicts,
  keyBindings,
  parseKeyBindings,
  serializeKeyBindings,
  type KeyBindingOverrides,
  type KeyBindings,
} from "./bindings";
export { useShortcutDispatcher } from "./dispatcher";
export { requestFocus, useFocusTarget } from "./focus-targets";
export { useActions, useRegisterActions, useShortcut, useShortcutLookup } from "./hooks";
export {
  ACTION_INFO,
  ADD_GROUPS,
  ActionIds,
  DEFAULT_SHORTCUTS,
  isWellKnownAction,
  type WellKnownActionId,
} from "./keymap";
export { useMenuCommands } from "./menu";
export { isModalOpen, pushModal } from "./modal";
export { usePlatform, type Platform } from "./platform";
export {
  actions,
  setActionErrorHandler,
  type Action,
  type ActionSpec,
} from "./registry";
export { runScoped, scopedHandler, useCommandScope } from "./scopes";
export {
  shortcutFromEvent,
  shortcutAriaLabel,
  shortcutLabel,
} from "./shortcut";
