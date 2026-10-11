/**
 * Modal dialogs: `Dialog` (the frame), `ConfirmDialog` (Discard changes, Delete), and `PromptDialog`
 * (a name, e.g. New folder).
 *
 * A dialog is placed on the window (Add, Preferences, Project settings) or on the canvas area
 * (`#content-area`: Publish, Discard, Delete, Rename), with its scrim over just that area.
 * While open, it traps Tab inside itself, makes the app behind it inert, silences window shortcuts,
 * closes on Escape, and returns focus to where it was when it closes.
 */

import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode, type RefObject, type SyntheticEvent } from "react";
import { Button } from "./Button";
import { IconButton } from "./IconButton";
import { cx, tabbableElements, trapTab } from "./internal/dom";
import { Portal, useDialogFooter, useModal } from "./internal/layers";
import { TextField } from "./TextField";
import "./Dialog.css";

/** The canvas area's element id; the frame (app/) gives it to the area beside the sidebar. */
const CANVAS_AREA = "content-area";

export interface DialogProps {
  readonly open: boolean;
  /** Escape, the scrim, the close button, and Cancel buttons call this. */
  readonly onClose: () => void;
  readonly title: string;
  /** Under the title; it becomes the dialog's accessible description. */
  readonly description?: ReactNode;
  /**
   * 400 for confirmations, 520 for Publish, 880 for Project settings and 1000 for the launch editor
   * (those two with a close button and ruled header and footer).
   */
  readonly width: 400 | 520 | 880 | 1000;
  /** A fixed height in px (Project settings: 700); otherwise it fits its content. */
  readonly height?: number;
  /** `window`: centered on the window, scrim over everything. `canvas`: centered on the canvas area, scrim over it alone. */
  readonly placement: "window" | "canvas";
  /** Buttons, right-aligned at the bottom. */
  readonly footer?: ReactNode;
  /** A control in the header, before the close button (the launch editor's kind switch). */
  readonly headerAction?: ReactNode;
  /** What takes focus when the dialog opens; defaults to the first focusable element. */
  readonly initialFocus?: RefObject<HTMLElement | null>;
  /** `alertdialog` for confirmations that interrupt. */
  readonly role?: "dialog" | "alertdialog";
  readonly className?: string;
  readonly children?: ReactNode;
}

/** Keeps a canvas-placed scrim over `#content-area` (the whole window when there is none). */
function useCanvasArea(scrim: RefObject<HTMLDivElement | null>, active: boolean): void {
  useLayoutEffect(() => {
    const element = scrim.current;
    const area = document.getElementById(CANVAS_AREA);
    if (!active || !element || !area) {
      return undefined;
    }
    const update = () => {
      const rect = area.getBoundingClientRect();
      Object.assign(element.style, { left: `${rect.left}px`, top: `${rect.top}px`, width: `${rect.width}px`, height: `${rect.height}px` });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(area);
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", update);
    };
  }, [scrim, active]);
}

export function Dialog({
  open,
  onClose,
  title,
  description,
  width,
  height,
  placement,
  footer,
  headerAction,
  initialFocus,
  role = "dialog",
  className,
  children,
}: DialogProps) {
  const id = useId();
  const scrim = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const footerRef = useRef<HTMLDivElement>(null);
  useModal(open);
  useDialogFooter(footerRef, open && footer !== undefined);
  useCanvasArea(scrim, open && placement === "canvas");

  useEffect(() => {
    const element = panel.current;
    if (open && element) {
      (initialFocus?.current ?? tabbableElements(element)[0] ?? element).focus();
    }
  }, [open, initialFocus]);

  if (!open) {
    return null;
  }
  const large = width === 880 || width === 1000;
  return (
    <Portal>
      <div
        ref={scrim}
        className={cx("dialog-scrim", `dialog-scrim-${placement}`)}
        onPointerDown={(event) => {
          // Only a press on the scrim itself, not one that started inside the panel.
          if (event.target === event.currentTarget) {
            onClose();
          }
        }}
      >
        <div
          ref={panel}
          role={role}
          aria-modal="true"
          aria-labelledby={`${id}-title`}
          aria-describedby={description === undefined ? undefined : `${id}-description`}
          tabIndex={-1}
          className={cx("dialog", `dialog-${width}`, className)}
          style={height === undefined ? undefined : { height }}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              onClose();
              return;
            }
            trapTab(event, event.currentTarget);
          }}
        >
          <div className="dialog-header">
            <div className="dialog-heading">
              <h2 id={`${id}-title`} className="dialog-title">
                {title}
              </h2>
              {description === undefined ? null : (
                <div id={`${id}-description`} className="dialog-description">
                  {description}
                </div>
              )}
            </div>
            {headerAction}
            {large ? <IconButton icon="close" label="Close" onClick={onClose} /> : null}
          </div>
          {children === undefined ? null : <div className="dialog-body">{children}</div>}
          {footer === undefined ? null : (
            <div ref={footerRef} className="dialog-footer">
              {footer}
            </div>
          )}
        </div>
      </div>
    </Portal>
  );
}

export interface ConfirmDialogProps {
  readonly open: boolean;
  readonly title: string;
  readonly message: ReactNode;
  readonly confirmLabel: string;
  readonly cancelLabel?: string;
  /** Destructive confirmations get a danger button, and focus starts on Cancel. */
  readonly danger?: boolean;
  readonly placement: "window" | "canvas";
  readonly onConfirm: () => void;
  readonly onCancel: () => void;
}

/** Asks one yes-or-no question, 400 wide. */
export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel = "Cancel",
  danger = false,
  placement,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const cancel = useRef<HTMLButtonElement>(null);
  const confirm = useRef<HTMLButtonElement>(null);
  return (
    <Dialog
      open={open}
      onClose={onCancel}
      title={title}
      description={message}
      width={400}
      placement={placement}
      role="alertdialog"
      initialFocus={danger ? cancel : confirm}
      footer={
        <>
          <Button ref={cancel} variant="secondary" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button ref={confirm} variant={danger ? "danger" : "primary"} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </>
      }
    />
  );
}

export interface PromptDialogProps {
  readonly open: boolean;
  readonly title: string;
  readonly description?: ReactNode;
  /** The field's label. */
  readonly label: string;
  readonly defaultValue?: string;
  readonly confirmLabel: string;
  readonly cancelLabel?: string;
  /** Returns a message when the value can't be used; the confirm button stays enabled. */
  readonly validate?: (value: string) => string | null;
  readonly placement: "window" | "canvas";
  readonly onSubmit: (value: string) => void;
  readonly onCancel: () => void;
}

interface PromptBodyProps {
  readonly formId: string;
  readonly field: RefObject<HTMLInputElement | null>;
  readonly label: string;
  readonly defaultValue: string;
  readonly validate: ((value: string) => string | null) | undefined;
  readonly onSubmit: (value: string) => void;
}

/** The prompt's form. It mounts each time the dialog opens, so every opening starts fresh. */
function PromptBody({ formId, field, label, defaultValue, validate, onSubmit }: PromptBodyProps) {
  const [value, setValue] = useState(defaultValue);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    field.current?.select();
  }, [field]);

  const submit = (event: SyntheticEvent) => {
    event.preventDefault();
    const problem = validate?.(value) ?? null;
    setError(problem);
    if (problem === null) {
      onSubmit(value);
    }
  };

  return (
    <form id={formId} onSubmit={submit} noValidate>
      <TextField
        ref={field}
        label={label}
        value={value}
        error={error}
        onChange={(event) => {
          setValue(event.target.value);
          setError(null);
        }}
      />
    </form>
  );
}

/** Asks for one line of text, 400 wide. The text starts selected; Enter submits. */
export function PromptDialog({
  open,
  title,
  description,
  label,
  defaultValue = "",
  confirmLabel,
  cancelLabel = "Cancel",
  validate,
  placement,
  onSubmit,
  onCancel,
}: PromptDialogProps) {
  const field = useRef<HTMLInputElement>(null);
  const formId = useId();
  return (
    <Dialog
      open={open}
      onClose={onCancel}
      title={title}
      {...(description === undefined ? {} : { description })}
      width={400}
      placement={placement}
      initialFocus={field}
      footer={
        <>
          <Button variant="secondary" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button variant="primary" type="submit" form={formId}>
            {confirmLabel}
          </Button>
        </>
      }
    >
      <PromptBody
        formId={formId}
        field={field}
        label={label}
        defaultValue={defaultValue}
        validate={validate}
        onSubmit={onSubmit}
      />
    </Dialog>
  );
}
