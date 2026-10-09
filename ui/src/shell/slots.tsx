/**
 * Named places in the window chrome that other parts of the UI fill: the board's title, Publish,
 * and menu in the title bar, and the zoom readout in the status bar. The chrome renders a
 * `<ShellSlot name>`; the owner renders `<SlotPortal name>…</SlotPortal>` anywhere in its own tree,
 * so its content keeps its React context and events.
 */

import { useEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { create } from "zustand";

export type ShellSlotName = "titlebar.board" | "titlebar.actions" | "titlebar.menu" | "statusbar.zoom";

const useSlots = create<Partial<Record<ShellSlotName, HTMLElement>>>()(() => ({}));

/** A place in the chrome. Empty until something portals into it. */
export function ShellSlot({ name, className }: { readonly name: ShellSlotName; readonly className?: string }) {
  const [element, setElement] = useState<HTMLElement | null>(null);
  useEffect(() => {
    if (!element) {
      return undefined;
    }
    useSlots.setState({ [name]: element });
    return () => {
      if (useSlots.getState()[name] === element) {
        useSlots.setState({ [name]: undefined });
      }
    };
  }, [name, element]);
  return <div ref={setElement} className={className} data-slot={name} />;
}

/** Renders `children` into the named slot while both are mounted. */
export function SlotPortal({ name, children }: { readonly name: ShellSlotName; readonly children: ReactNode }) {
  const target = useSlots((state) => state[name]);
  return target ? createPortal(children, target) : null;
}
