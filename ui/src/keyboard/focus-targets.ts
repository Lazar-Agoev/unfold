/**
 * Named focus targets, so an action can focus a field that may not be mounted yet: "Focus search"
 * (`/`) shows the Search view, and its field takes focus as soon as it mounts.
 */

import { useEffect, type RefObject } from "react";

const targets = new Map<string, HTMLElement>();
let pending: string | null = null;

/** Focuses the target named `key` now, or as soon as it mounts. */
export function requestFocus(key: string): void {
  const element = targets.get(key);
  if (element?.isConnected) {
    pending = null;
    element.focus();
    if (element instanceof HTMLInputElement) {
      element.select();
    }
  } else {
    pending = key;
  }
}

/** Registers the element behind `ref` as the focus target `key` while mounted. */
export function useFocusTarget(key: string | undefined, ref: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const element = ref.current;
    if (key === undefined || !element) {
      return undefined;
    }
    targets.set(key, element);
    if (pending === key) {
      requestFocus(key);
    }
    return () => {
      if (targets.get(key) === element) {
        targets.delete(key);
      }
    };
  }, [key, ref]);
}
