/**
 * Text inputs: `TextField` (a labeled field with hint and error) and `SearchField` (a filter box with
 * a clear button; Escape clears it). A SearchField with a `focusKey` is the target of actions such as
 * "Focus search" (`/`), even if it mounts after the action runs.
 */

import { useId, useRef, type ChangeEvent, type InputHTMLAttributes, type KeyboardEvent, type Ref } from "react";
import { Icon } from "../icons";
import { useFocusTarget } from "../keyboard";
import { assignRef, cx } from "./internal/dom";
import "./TextField.css";

export interface TextFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "size"> {
  readonly label: string;
  /** Keeps the label for screen readers only. */
  readonly hideLabel?: boolean;
  readonly hint?: string;
  /** Shown under the field and marks it invalid. */
  readonly error?: string | null;
  readonly ref?: Ref<HTMLInputElement>;
}

export function TextField({ label, hideLabel = false, hint, error, className, id, ref, ...rest }: TextFieldProps) {
  const generated = useId();
  const inputId = id ?? generated;
  const describedBy = [error ? `${inputId}-error` : null, hint ? `${inputId}-hint` : null].filter(Boolean).join(" ");
  return (
    <div className={cx("text-field", className)}>
      <label htmlFor={inputId} className={cx("text-field-label", hideLabel && "visually-hidden")}>
        {label}
      </label>
      <input
        {...rest}
        ref={ref}
        id={inputId}
        className="text-field-input"
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy || undefined}
        spellCheck={false}
        autoComplete="off"
      />
      {error ? (
        <p id={`${inputId}-error`} className="text-field-error">
          {error}
        </p>
      ) : null}
      {hint ? (
        <p id={`${inputId}-hint`} className="text-field-hint">
          {hint}
        </p>
      ) : null}
    </div>
  );
}

export interface SearchFieldProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type" | "size"> {
  readonly value: string;
  readonly onChange: (value: string) => void;
  /** The accessible name (the field has no visible label). */
  readonly label: string;
  /** Registers the field as a named focus target (see `requestFocus`). */
  readonly focusKey?: string;
  readonly ref?: Ref<HTMLInputElement>;
}

export function SearchField({ value, onChange, label, focusKey, className, onKeyDown, ref, ...rest }: SearchFieldProps) {
  const input = useRef<HTMLInputElement | null>(null);
  useFocusTarget(focusKey, input);

  const setRefs = (element: HTMLInputElement | null) => {
    input.current = element;
    assignRef(ref, element);
  };

  return (
    <div className={cx("search-field", className)}>
      <Icon name="search" size={14} className="search-field-icon" />
      <input
        {...rest}
        ref={setRefs}
        type="text"
        role="searchbox"
        aria-label={label}
        className="search-field-input"
        value={value}
        spellCheck={false}
        autoComplete="off"
        onChange={(event: ChangeEvent<HTMLInputElement>) => {
          onChange(event.target.value);
        }}
        onKeyDown={(event: KeyboardEvent<HTMLInputElement>) => {
          onKeyDown?.(event);
          if (event.defaultPrevented || event.key !== "Escape") {
            return;
          }
          // Escape clears a filled field, and leaves an empty one.
          event.preventDefault();
          if (value === "") {
            event.currentTarget.blur();
          } else {
            onChange("");
          }
        }}
      />
      {value === "" ? null : (
        <button
          type="button"
          className="search-field-clear"
          aria-label="Clear search"
          // Escape clears from the keyboard, so the button stays out of the Tab order.
          tabIndex={-1}
          onClick={() => {
            onChange("");
            input.current?.focus();
          }}
        >
          <Icon name="close" size={12} />
        </button>
      )}
    </div>
  );
}
