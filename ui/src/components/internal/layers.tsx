/** Portals and dismissal for floating layers. */

import { useEffect, useLayoutEffect, useRef, useSyncExternalStore, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";

/** Renders children at the end of <body>, above the shell. */
export function Portal({ children }: { children: ReactNode }) {
  return createPortal(children, document.body);
}

/**
 * Calls `onDismiss` when the person presses outside the layer (and outside `ignore`, usually the
 * anchor) or the window loses focus. Escape is handled by each layer's own keydown, so the innermost
 * layer closes first.
 */
export function useDismiss(
  layer: RefObject<HTMLElement | null>,
  open: boolean,
  onDismiss: () => void,
  ignore?: RefObject<HTMLElement | null>,
): void {
  const latest = useRef(onDismiss);
  useEffect(() => {
    latest.current = onDismiss;
  });
  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target instanceof Node ? event.target : null;
      if (target && (layer.current?.contains(target) || ignore?.current?.contains(target))) {
        return;
      }
      latest.current();
    };
    const onBlur = () => {
      latest.current();
    };
    document.addEventListener("pointerdown", onPointerDown, { capture: true });
    window.addEventListener("blur", onBlur);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, { capture: true });
      window.removeEventListener("blur", onBlur);
    };
  }, [open, layer, ignore]);
}

/**
 * Returns focus to whatever had it before `active` became true, when it becomes false again. A
 * layout effect, so it records focus before any child's (passive) effect moves it into the layer.
 */
export function useRestoreFocus(active: boolean): void {
  useLayoutEffect(() => {
    if (!active) {
      return undefined;
    }
    const previous = document.activeElement;
    return () => {
      if (previous instanceof HTMLElement && previous.isConnected) {
        previous.focus({ preventScroll: true });
      }
    };
  }, [active]);
}

/** How many modal layers are open; the app root is inert while any is. */
let openModals = 0;

/** Whether a modal dialog is open (the board behind it then ignores Edit › Undo and Redo). */
export function isModalOpen(): boolean {
  return openModals > 0;
}

/**
 * For modal layers (dialogs), while `open`: make the app behind inert. On close, the app becomes interactive again before focus returns to where it
 * was (focus can't land inside an inert subtree).
 */
export function useModal(open: boolean): void {
  // A layout effect, for the same reason as `useRestoreFocus`.
  useLayoutEffect(() => {
    if (!open) {
      return undefined;
    }
    const previous = document.activeElement;
    const root = document.getElementById("root");
    openModals += 1;
    root?.setAttribute("inert", "");
    return () => {
      openModals -= 1;
      if (openModals === 0) {
        root?.removeAttribute("inert");
      }
      if (previous instanceof HTMLElement && previous.isConnected) {
        previous.focus({ preventScroll: true });
      }
    };
  }, [open]);
}

/** The footers of the open dialogs, innermost last; the toaster keeps clear of their buttons. */
let dialogFooters: readonly HTMLElement[] = [];
const footerListeners = new Set<() => void>();

function setDialogFooters(next: readonly HTMLElement[]): void {
  dialogFooters = next;
  for (const listener of footerListeners) {
    listener();
  }
}

/** Registers an open dialog's footer (`present` while it has one) for `useDialogFooters`. */
export function useDialogFooter(footer: RefObject<HTMLElement | null>, present: boolean): void {
  useLayoutEffect(() => {
    const element = footer.current;
    if (!present || !element) {
      return undefined;
    }
    setDialogFooters([...dialogFooters, element]);
    return () => {
      setDialogFooters(dialogFooters.filter((item) => item !== element));
    };
  }, [footer, present]);
}

function subscribeFooters(listener: () => void): () => void {
  footerListeners.add(listener);
  return () => {
    footerListeners.delete(listener);
  };
}

/** The footers of the open dialogs, innermost last. */
export function useDialogFooters(): readonly HTMLElement[] {
  return useSyncExternalStore(subscribeFooters, () => dialogFooters);
}
