/**
 * Toasts: short notes in the window's bottom-right corner that never block the canvas
 * ("Copied the link", "Exported PNG"). Call `showToast` from anywhere; the
 * frame renders one `<Toaster />`. At most four show, newest at the bottom; a toast shown with the id
 * of one already showing replaces it in place. While a dialog is open, the stack rises above its
 * footer when the two would meet, so its buttons stay reachable.
 */

import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { create } from "zustand";
import { Icon } from "../icons";
import { TextButton } from "./Button";
import { IconButton } from "./IconButton";
import { cx } from "./internal/dom";
import { Portal, useDialogFooters } from "./internal/layers";
import "./Toast.css";

export interface ToastOptions {
  readonly title: string;
  readonly body?: string;
  /** The leading slot: a progress ring, a monogram. Danger toasts default to a warning icon. */
  readonly leading?: ReactNode;
  /** One action link, e.g. "Undo". Choosing it dismisses the toast. */
  readonly action?: { readonly label: string; readonly run: () => void };
  /** Milliseconds before it hides itself; null keeps it until dismissed. Default 5 s, 8 s for danger. */
  readonly duration?: number | null;
  readonly tone?: "info" | "danger";
  /** Showing a toast with the id of one on screen replaces that one (the engine toast). */
  readonly id?: string;
}

interface ShownToast extends ToastOptions {
  readonly id: string;
  /** Changes on every show, so a replaced toast restarts its timer. */
  readonly shownAt: number;
}

const useToasts = create<{ readonly toasts: readonly ShownToast[] }>()(() => ({ toasts: [] }));
let nextId = 1;
let shows = 0;

const MAX_TOASTS = 4;
/** The stack's distance from the window's right and bottom edges, and the space it keeps above a dialog's footer. */
const EDGE = 24;
const GAP = 10;

/** Shows a toast and returns its id. */
export function showToast(options: ToastOptions): string {
  shows += 1;
  const toast: ShownToast = { ...options, id: options.id ?? `toast-${nextId++}`, shownAt: shows };
  useToasts.setState((state) => {
    const index = state.toasts.findIndex((shown) => shown.id === toast.id);
    if (index >= 0) {
      return { toasts: state.toasts.map((shown, at) => (at === index ? toast : shown)) };
    }
    return { toasts: [...state.toasts, toast].slice(-MAX_TOASTS) };
  });
  return toast.id;
}

export function dismissToast(id: string): void {
  useToasts.setState((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
}

function ToastItem({ toast }: { readonly toast: ShownToast }) {
  const tone = toast.tone ?? "info";
  const duration = toast.duration === undefined ? (tone === "danger" ? 8000 : 5000) : toast.duration;
  // Hovering or focusing a toast pauses its timer, so it never vanishes while being read.
  const [paused, setPaused] = useState(false);
  useEffect(() => {
    if (duration === null || paused) {
      return undefined;
    }
    const timer = window.setTimeout(() => {
      dismissToast(toast.id);
    }, duration);
    return () => {
      window.clearTimeout(timer);
    };
  }, [duration, paused, toast.id, toast.shownAt]);
  const leading = toast.leading ?? (tone === "danger" ? <Icon name="warning-circle" className="toast-danger-icon" /> : null);

  return (
    <div
      className={cx("toast", `toast-${tone}`)}
      // The toaster is a polite live region; failures interrupt as alerts.
      role={tone === "danger" ? "alert" : undefined}
      onPointerEnter={() => {
        setPaused(true);
      }}
      onPointerLeave={() => {
        setPaused(false);
      }}
      onFocus={() => {
        setPaused(true);
      }}
      onBlur={() => {
        setPaused(false);
      }}
    >
      {leading === null ? null : <div className="toast-leading">{leading}</div>}
      <div className="toast-text">
        <p className="toast-title">{toast.title}</p>
        {toast.body === undefined ? null : <p className="toast-body">{toast.body}</p>}
      </div>
      {toast.action ? (
        <TextButton
          className="toast-action"
          onClick={() => {
            dismissToast(toast.id);
            toast.action?.run();
          }}
        >
          {toast.action.label}
        </TextButton>
      ) : null}
      <IconButton
        icon="close"
        label="Dismiss"
        size={20}
        className="toast-close"
        onClick={() => {
          dismissToast(toast.id);
        }}
      />
    </div>
  );
}

/**
 * Lifts the stack just above the innermost open dialog's footer while the footer would sit under it
 * (in its bottom right corner, sized as it is); otherwise it keeps its place.
 */
function useFooterClearance(stack: RefObject<HTMLElement | null>): void {
  const footers = useDialogFooters();
  useLayoutEffect(() => {
    const element = stack.current;
    const footer = footers.at(-1);
    if (!element || !footer) {
      return undefined;
    }
    const update = () => {
      const area = footer.getBoundingClientRect();
      const right = window.innerWidth - EDGE;
      const base = window.innerHeight - EDGE;
      const meets =
        element.offsetHeight > 0 &&
        area.left < right &&
        area.right > right - element.offsetWidth &&
        area.top < base &&
        area.bottom > base - element.offsetHeight;
      Object.assign(element.style, { bottom: meets ? `${window.innerHeight - area.top + GAP}px` : "" });
    };
    update();
    // The footer moves when its dialog resizes, and the stack grows and shrinks with its toasts.
    const observer = new ResizeObserver(update);
    observer.observe(element);
    observer.observe(footer.parentElement ?? footer);
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", update);
      Object.assign(element.style, { bottom: "" });
    };
  }, [footers, stack]);
}

/** Renders the window's toasts, bottom right, above everything else (and clear of a dialog's buttons). */
export function Toaster() {
  const toasts = useToasts((state) => state.toasts);
  const stack = useRef<HTMLElement>(null);
  useFooterClearance(stack);
  return (
    <Portal>
      <section ref={stack} className="toaster" aria-label="Notifications" aria-live="polite">
        {toasts.map((toast) => (
          <ToastItem key={toast.id} toast={toast} />
        ))}
      </section>
    </Portal>
  );
}
