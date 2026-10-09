/**
 * Toasts: short, transient status messages ("Board published", "Couldn't open the folder"). Call
 * `showToast` from anywhere; the shell renders one `<Toaster />`.
 */

import { CircleAlert, CircleCheck, Info, X } from "lucide-react";
import { useEffect, useState } from "react";
import { create } from "zustand";
import { cx } from "./internal/dom";
import { Portal } from "./internal/layers";
import "./Toast.css";

export type ToastTone = "info" | "success" | "danger";

export interface ToastOptions {
  readonly message: string;
  readonly tone?: ToastTone;
  /** Milliseconds before it hides itself; null keeps it until dismissed. Default 4 s (8 s for danger). */
  readonly duration?: number | null;
  /** One optional action button, e.g. "Undo". */
  readonly action?: { readonly label: string; readonly run: () => void };
}

export interface Toast extends ToastOptions {
  readonly id: number;
}

interface ToastState {
  readonly toasts: readonly Toast[];
}

const useToasts = create<ToastState>()(() => ({ toasts: [] }));
let nextId = 1;

/** At most this many toasts show at once; older ones make room. */
const MAX_TOASTS = 4;

/** Shows a toast and returns its id. */
export function showToast(options: ToastOptions | string): number {
  const toast: Toast = { ...(typeof options === "string" ? { message: options } : options), id: nextId++ };
  useToasts.setState((state) => ({ toasts: [...state.toasts, toast].slice(-MAX_TOASTS) }));
  return toast.id;
}

export function dismissToast(id: number): void {
  useToasts.setState((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
}

const ICONS = { info: Info, success: CircleCheck, danger: CircleAlert } as const;

function ToastItem({ toast }: { toast: Toast }) {
  const tone = toast.tone ?? "info";
  const duration = toast.duration === undefined ? (tone === "danger" ? 8000 : 4000) : toast.duration;
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
  }, [duration, paused, toast.id]);
  const Icon = ICONS[tone];

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
      <Icon className="toast-icon" size={16} aria-hidden="true" />
      <p className="toast-message">{toast.message}</p>
      {toast.action ? (
        <button
          type="button"
          className="toast-action"
          onClick={() => {
            dismissToast(toast.id);
            toast.action?.run();
          }}
        >
          {toast.action.label}
        </button>
      ) : null}
      <button
        type="button"
        className="toast-close"
        aria-label="Dismiss"
        onClick={() => {
          dismissToast(toast.id);
        }}
      >
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  );
}

/** Renders the window's toasts, bottom right, above everything else. */
export function Toaster() {
  const toasts = useToasts((state) => state.toasts);
  return (
    <Portal>
      <section className="toaster" aria-label="Notifications" aria-live="polite">
        {toasts.map((toast) => (
          <ToastItem key={toast.id} toast={toast} />
        ))}
      </section>
    </Portal>
  );
}
