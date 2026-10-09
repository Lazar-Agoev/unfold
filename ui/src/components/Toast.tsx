/**
 * Toasts (07, V-67): short notes in the window's bottom-right corner that never block the canvas
 * ("storefront-web compiled", "Reconnecting to the engine…"). Call `showToast` from anywhere; the
 * frame renders one `<Toaster />`. At most four show, newest at the bottom; a toast shown with the id
 * of one already showing replaces it in place.
 */

import { useEffect, useState, type ReactNode } from "react";
import { create } from "zustand";
import { Icon } from "../icons";
import { TextButton } from "./Button";
import { IconButton } from "./IconButton";
import { cx } from "./internal/dom";
import { Portal } from "./internal/layers";
import "./Toast.css";

export interface ToastOptions {
  readonly title: string;
  readonly body?: string;
  /** The leading slot: a progress ring, a monogram. Danger toasts default to a warning icon. */
  readonly leading?: ReactNode;
  /** One action link, e.g. "Open Home". Choosing it dismisses the toast. */
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
