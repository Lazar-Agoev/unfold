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
export { dispatchShortcut, useShortcutDispatcher, type DispatchContext } from "./dispatcher";
export { requestFocus, useFocusTarget } from "./focus-targets";
export { useActions, useRegisterActions, useShortcut, useShortcutLabel, useShortcutLookup } from "./hooks";
export {
  ACTION_INFO,
  ADD_GROUPS,
  ActionIds,
  DEFAULT_SHORTCUTS,
  MENU_COMMAND_ACTIONS,
  defaultShortcuts,
  isWellKnownAction,
  type WellKnownActionId,
} from "./keymap";
export { runMenuCommand, useMenuCommands } from "./menu";
export { isModalOpen, pushModal } from "./modal";
export { currentPlatform, usePlatform, type Platform } from "./platform";
export {
  actions,
  createActionRegistry,
  setActionErrorHandler,
  type Action,
  type ActionRegistry,
  type ActionSpec,
} from "./registry";
export { registerScope, runScoped, scopedHandler, useCommandScope, type ScopeHandlers, type ScopedCommand } from "./scopes";
export {
  chordIdentity,
  isEditableTarget,
  matchesChord,
  normalizeShortcut,
  parseShortcut,
  shortcutFromEvent,
  shortcutAriaLabel,
  shortcutLabel,
  type Chord,
} from "./shortcut";
