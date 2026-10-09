/**
 * Modal dialogs: `Dialog` (the frame), `ConfirmDialog` (Delete board, conflict prompts), and
 * `PromptDialog` (a name, e.g. Publish when the name is taken).
 *
 * While open, a dialog traps Tab inside itself, makes the app behind it inert, silences window
 * shortcuts, closes on Escape, and returns focus to where it was when it closes.
 */

import { useEffect, useId, useRef, useState, type SyntheticEvent, type ReactNode, type RefObject } from "react";
import { Button } from "./Button";
import { cx, tabbableElements, trapTab } from "./internal/dom";
import { Portal, useModal } from "./internal/layers";
import { TextField } from "./TextField";
import "./Dialog.css";

export interface DialogProps {
  readonly open: boolean;
  /** Escape, the backdrop, and Cancel buttons call this. */
  readonly onClose: () => void;
  readonly title: string;
  /** A sentence under the title; it becomes the dialog's accessible description. */
  readonly description?: ReactNode;
  /** `alertdialog` for confirmations that interrupt. */
  readonly role?: "dialog" | "alertdialog";
  /** What takes focus when the dialog opens; defaults to the first focusable element. */
  readonly initialFocus?: RefObject<HTMLElement | null>;
  /** Buttons, right-aligned at the bottom. */
  readonly footer?: ReactNode;
  readonly width?: "small" | "medium";
  readonly className?: string;
  readonly children?: ReactNode;
}

export function Dialog({
  open,
  onClose,
  title,
  description,
  role = "dialog",
  initialFocus,
  footer,
  width = "small",
  className,
  children,
}: DialogProps) {
  const id = useId();
  const panel = useRef<HTMLDivElement>(null);
  useModal(open);

  useEffect(() => {
    const element = panel.current;
    if (open && element) {
      (initialFocus?.current ?? tabbableElements(element)[0] ?? element).focus();
    }
  }, [open, initialFocus]);

  if (!open) {
    return null;
  }
  return (
    <Portal>
      <div
        className="dialog-backdrop"
        onPointerDown={(event) => {
          // Only a press on the backdrop itself, not one that started inside the panel.
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
          <h2 id={`${id}-title`} className="dialog-title">
            {title}
          </h2>
          {description === undefined ? null : (
            <div id={`${id}-description`} className="dialog-description">
              {description}
            </div>
          )}
          {children === undefined ? null : <div className="dialog-body">{children}</div>}
          {footer === undefined ? null : <div className="dialog-footer">{footer}</div>}
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
  readonly onConfirm: () => void;
  readonly onCancel: () => void;
}

/** Asks one yes-or-no question. */
export function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel = "Cancel",
  danger = false,
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
      role="alertdialog"
      initialFocus={danger ? cancel : confirm}
      footer={
        <>
          <Button ref={cancel} onClick={onCancel}>
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

/** Asks for one line of text. The text starts selected; Enter submits. */
export function PromptDialog({
  open,
  title,
  description,
  label,
  defaultValue = "",
  confirmLabel,
  cancelLabel = "Cancel",
  validate,
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
      initialFocus={field}
      footer={
        <>
          <Button onClick={onCancel}>{cancelLabel}</Button>
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
