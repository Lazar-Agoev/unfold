/**
 * The action registry: every command a person can run (by shortcut, menu, or Add's `>` commands) is an
 * action registered here. Views register actions while they are mounted; registering an id that is already
 * registered stacks on top of it until unregistered, so a focused view can take over `⌘W` or `⌘Z`.
 *
 * An action's shortcuts resolve as: the person's key binding for its id (see `bindings.ts`), else
 * the spec's own `shortcuts`, else the keymap's defaults. They re-resolve when the bindings change.
 */

import type { IconName } from "../icons";
import { keyBindings as windowBindings, type KeyBindings } from "./bindings";
import { defaultShortcuts } from "./keymap";

export interface ActionSpec {
  /** A stable dotted id, e.g. `view.toggleSidebar`. Well-known ids live in `ActionIds`. */
  readonly id: string;
  /** Title-case, shown in Add's commands and menus: "Toggle Sidebar". */
  readonly title: string;
  /** Where Preferences → Keyboard lists it: "View". */
  readonly category: string;
  /** The group Add's commands show it under ("Board ›"); defaults to the category. See `ADD_GROUPS`. */
  readonly group?: string;
  /** The icon Add's commands show beside it. */
  readonly icon?: IconName;
  /** Overrides the keymap's default shortcuts for this id; `[]` means none. A key binding still wins. */
  readonly shortcuts?: readonly string[];
  /** Extra words Add's commands match. */
  readonly keywords?: readonly string[];
  /**
   * Whether the shortcut fires while typing in a text field. Defaults to true for ⌘/Ctrl shortcuts
   * and false for plain keys (`/`, `]`, `Shift+1`).
   */
  readonly allowInInput?: boolean;
  /** Whether holding the keys repeats the action (zoom does; toggles don't). */
  readonly repeatable?: boolean;
  /** Hidden from Add's commands when false (default true). */
  readonly showInAdd?: boolean;
  /** Checked before every run; a disabled action lets its key through. */
  readonly enabled?: () => boolean;
  readonly run: () => void | Promise<void>;
}

/** A registered action, with its shortcuts resolved. */
export interface Action extends ActionSpec {
  readonly shortcuts: readonly string[];
  /** Registration order; later registrations win shortcut conflicts. */
  readonly order: number;
}

interface Entry {
  readonly spec: ActionSpec;
  readonly order: number;
}

export interface ActionRegistry {
  /** Registers one or more actions. Returns a function that unregisters exactly these. */
  readonly register: (specs: ActionSpec | readonly ActionSpec[]) => () => void;
  /** The action currently answering to `id` (the most recent registration). */
  readonly get: (id: string) => Action | undefined;
  /** Every id's current action, sorted by category then title. Stable until the registry changes. */
  readonly list: () => readonly Action[];
  /** An id's effective shortcuts: its action's if registered, else the binding or keymap default. */
  readonly shortcutsFor: (id: string) => readonly string[];
  readonly isEnabled: (id: string) => boolean;
  /** Runs an action if it is registered and enabled. Returns whether it ran. */
  readonly run: (id: string) => boolean;
  /** Observes registrations (for `useSyncExternalStore`). */
  readonly subscribe: (listener: () => void) => () => void;
}

export interface RegistryOptions {
  /** Called when an action throws or its promise rejects. */
  readonly onError?: (error: unknown, action: Action) => void;
  /** The person's key bindings; without them every action keeps its spec's or the keymap's shortcuts. */
  readonly bindings?: KeyBindings;
}

export function createActionRegistry(options: RegistryOptions = {}): ActionRegistry {
  const { bindings } = options;
  const stacks = new Map<string, Entry[]>();
  const resolved = new Map<string, Action>();
  const listeners = new Set<() => void>();
  let order = 0;
  let snapshot: readonly Action[] | null = null;

  const changed = () => {
    snapshot = null;
    resolved.clear();
    for (const listener of [...listeners]) {
      listener();
    }
  };
  bindings?.subscribe(changed);

  const reportError = (error: unknown, action: Action) => {
    if (options.onError) {
      options.onError(error, action);
    } else {
      console.error(`Action ${action.id} failed`, error);
    }
  };

  /** The top registration of `id`, resolved once per change so its identity is stable. */
  const get = (id: string): Action | undefined => {
    const cached = resolved.get(id);
    if (cached) {
      return cached;
    }
    const entry = stacks.get(id)?.at(-1);
    if (!entry) {
      return undefined;
    }
    const shortcuts = bindings?.override(id) ?? entry.spec.shortcuts ?? defaultShortcuts(id);
    const action: Action = { ...entry.spec, shortcuts, order: entry.order };
    resolved.set(id, action);
    return action;
  };

  const isEnabled = (id: string): boolean => {
    const action = get(id);
    return action !== undefined && (action.enabled?.() ?? true);
  };

  return {
    register: (specs) => {
      const added = ("id" in specs ? [specs] : specs).map((spec): Entry => ({ spec, order: ++order }));
      for (const entry of added) {
        const stack = stacks.get(entry.spec.id) ?? [];
        stack.push(entry);
        stacks.set(entry.spec.id, stack);
      }
      changed();
      return () => {
        for (const entry of added) {
          const stack = stacks.get(entry.spec.id);
          const index = stack?.indexOf(entry) ?? -1;
          if (stack && index >= 0) {
            stack.splice(index, 1);
            if (stack.length === 0) {
              stacks.delete(entry.spec.id);
            }
          }
        }
        changed();
      };
    },
    get,
    list: () => {
      snapshot ??= [...stacks.keys()]
        .map(get)
        .filter((action) => action !== undefined)
        .sort((a, b) => a.category.localeCompare(b.category) || a.title.localeCompare(b.title));
      return snapshot;
    },
    shortcutsFor: (id) => get(id)?.shortcuts ?? bindings?.override(id) ?? defaultShortcuts(id),
    isEnabled,
    run: (id) => {
      const action = get(id);
      if (!action || !(action.enabled?.() ?? true)) {
        return false;
      }
      // The async wrapper runs `run` synchronously up to its first await, and turns both a throw and
      // a rejected promise into one rejection.
      const running = (async () => {
        await action.run();
      })();
      running.catch((error: unknown) => {
        reportError(error, action);
      });
      return true;
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

let errorHandler: ((error: unknown, action: Action) => void) | null = null;

/** Where the window's actions report failures (the shell shows them as toasts). */
export function setActionErrorHandler(handler: ((error: unknown, action: Action) => void) | null): void {
  errorHandler = handler;
}

/** The window's registry. Each window is its own renderer, so a module-level registry is per window. */
export const actions: ActionRegistry = createActionRegistry({
  bindings: windowBindings,
  onError: (error, action) => {
    if (errorHandler) {
      errorHandler(error, action);
    } else {
      console.error(`Action ${action.id} failed`, error);
    }
  },
});
