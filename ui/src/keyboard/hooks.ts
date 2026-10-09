/** React hooks over the action registry. */

import { useEffect, useRef, useSyncExternalStore } from "react";
import { usePlatform } from "./platform";
import { actions as windowActions, type Action, type ActionRegistry, type ActionSpec } from "./registry";
import { shortcutLabel } from "./shortcut";

/** A string that changes when anything the registry stores about these specs (besides closures) changes. */
function signatureOf(specs: readonly ActionSpec[]): string {
  return JSON.stringify(
    specs.map((spec) => [
      spec.id,
      spec.title,
      spec.category,
      spec.group ?? null,
      spec.icon ?? null,
      spec.shortcuts ?? null,
      spec.keywords ?? null,
      spec.allowInInput ?? null,
      spec.repeatable ?? null,
      spec.showInAdd ?? null,
      spec.enabled !== undefined,
    ]),
  );
}

/**
 * Registers actions while the component is mounted. `run` and `enabled` may close over current
 * props and state: the registry calls the latest ones, so passing a fresh array each render is fine.
 */
export function useRegisterActions(specs: readonly ActionSpec[], registry: ActionRegistry = windowActions): void {
  const latest = useRef(specs);
  useEffect(() => {
    latest.current = specs;
  });
  const signature = signatureOf(specs);
  useEffect(() => {
    const proxies = latest.current.map((spec, index): ActionSpec => {
      const { enabled } = spec;
      return {
        ...spec,
        ...(enabled ? { enabled: () => latest.current[index]?.enabled?.() ?? false } : {}),
        run: () => latest.current[index]?.run(),
      };
    });
    return registry.register(proxies);
    // `signature` stands for the specs' registrable fields; closures are read through `latest`.
  }, [signature, registry]);
}

/** Every registered action, kept current. */
export function useActions(registry: ActionRegistry = windowActions): readonly Action[] {
  return useSyncExternalStore(registry.subscribe, registry.list);
}

/**
 * An action's primary effective shortcut (`Mod+Shift+P`, for `Kbd` and tooltips), or null if it has
 * none. Follows the person's key bindings, and works before the action is registered.
 */
export function useShortcut(id: string, registry: ActionRegistry = windowActions): string | null {
  return useSyncExternalStore(registry.subscribe, () => registry.shortcutsFor(id)[0] ?? null);
}

/**
 * `useShortcut` for a list of ids (one hook call for many buttons): a function from action id to its
 * primary effective shortcut, re-rendering when any shortcut changes.
 */
export function useShortcutLookup(registry: ActionRegistry = windowActions): (id: string) => string | null {
  // Re-render whenever registrations or bindings change, which is when lookups can change.
  useActions(registry);
  return (id: string) => registry.shortcutsFor(id)[0] ?? null;
}

/** The label of an action's primary shortcut in the platform's style (`⇧⌘P`), or null if it has none. */
export function useShortcutLabel(id: string, registry: ActionRegistry = windowActions): string | null {
  const platform = usePlatform();
  const shortcut = useShortcut(id, registry);
  return shortcut === null ? null : shortcutLabel(platform, shortcut);
}
