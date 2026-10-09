/** Small DOM helpers shared by the components. */

import type { Ref } from "react";

/** Joins class names, skipping falsy ones. */
export function cx(...names: readonly (string | false | null | undefined)[]): string {
  return names.filter((name) => typeof name === "string" && name.length > 0).join(" ");
}

const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
  "[contenteditable='true']",
].join(",");

/** The elements inside `container` that Tab reaches, in order. */
export function tabbableElements(container: HTMLElement): HTMLElement[] {
  return [...container.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) => !element.hasAttribute("inert") && element.closest("[inert]") === null && !element.hidden,
  );
}

/** Keeps Tab and Shift+Tab inside `container`. Call from its keydown handler. */
export function trapTab(event: Pick<KeyboardEvent, "key" | "shiftKey" | "preventDefault">, container: HTMLElement): void {
  if (event.key !== "Tab") {
    return;
  }
  const tabbable = tabbableElements(container);
  const first = tabbable[0];
  const last = tabbable.at(-1);
  if (!first || !last) {
    event.preventDefault();
    container.focus();
    return;
  }
  const active = document.activeElement;
  if (event.shiftKey && (active === first || !container.contains(active))) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && (active === last || !container.contains(active))) {
    event.preventDefault();
    first.focus();
  }
}

/** A point on screen, for menus opened at the pointer. */
export interface Point {
  readonly x: number;
  readonly y: number;
}

/** Where a context menu opened from the keyboard (Shift+F10, the Menu key) should appear for `element`. */
export function pointBelow(element: Element): Point {
  const rect = element.getBoundingClientRect();
  return { x: rect.left + Math.min(24, rect.width / 2), y: rect.bottom };
}

/** Whether a key opens a context menu from the keyboard. */
export function isContextMenuKey(event: Pick<KeyboardEvent, "key" | "shiftKey">): boolean {
  return event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
}

/** Sets a React ref of either kind (used when a component needs the element too). */
export function assignRef<T>(ref: Ref<T> | undefined, value: T | null): void {
  if (typeof ref === "function") {
    ref(value);
  } else if (ref) {
    ref.current = value;
  }
}
