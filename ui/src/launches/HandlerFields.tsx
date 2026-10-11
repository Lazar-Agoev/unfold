/**
 * "Handles a route, queue, or event": the entrance's kind, method (HTTP), trigger, and its handler
 * locators in the order they run, middleware first; and the locator forms beside them.
 */

import { useId, type Ref } from "react";
import { Button, IconButton } from "../components";
import type { HttpMethod } from "../ipc";
import { Icon } from "../icons";
import { HANDLER_KINDS, HTTP_METHODS, type DraftProblems, type HandlerKind, type LaunchDraft } from "./draft";
import { Field } from "./Field";
import { LOCATOR_FORMS } from "./locators";

interface HandlerFieldsProps {
  readonly draft: LaunchDraft;
  readonly problems: DraftProblems;
  /** Whether a field's problem shows yet (it was left, or Save was pressed). */
  readonly shown: (field: string) => boolean;
  readonly touch: (field: string) => () => void;
  readonly change: (next: Partial<LaunchDraft>) => void;
  /** The trigger field, which the dialog focuses first. */
  readonly triggerRef: Ref<HTMLInputElement>;
}

/** The kind, method, trigger, and handlers; the label, application, and description come from the editor. */
export function HandlerFields({ draft, problems, shown, touch, change, triggerRef }: HandlerFieldsProps) {
  const id = useId();
  const kind = HANDLER_KINDS.find((candidate) => candidate.value === draft.kind);
  const setHandler = (position: number, value: string) => {
    change({ handlers: draft.handlers.map((handler, at) => (at === position ? value : handler)) });
  };
  return (
    <>
      <div className="launch-pair">
        <Field label="Kind" htmlFor={`${id}-kind`} className="launch-grow">
          <div className="launch-select launch-select-native">
            <select
              id={`${id}-kind`}
              value={draft.kind}
              onChange={(event) => {
                change({ kind: event.target.value as HandlerKind });
              }}
            >
              {HANDLER_KINDS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            <Icon name="chevron-down-small" size={10} />
          </div>
        </Field>
        {draft.kind === "http" ? (
          <Field label="Method" htmlFor={`${id}-method`} className="launch-method">
            <div className="launch-select launch-select-native">
              <select
                id={`${id}-method`}
                value={draft.method}
                onChange={(event) => {
                  change({ method: event.target.value as HttpMethod | "" });
                }}
              >
                <option value="">Any or unknown</option>
                {HTTP_METHODS.map((method) => (
                  <option key={method} value={method}>
                    {method}
                  </option>
                ))}
              </select>
              <Icon name="chevron-down-small" size={10} />
            </div>
          </Field>
        ) : null}
      </div>
      <Field label="Trigger" htmlFor={`${id}-trigger`} aside="optional" error={shown("trigger") ? problems.trigger : undefined}>
        <input
          ref={triggerRef}
          id={`${id}-trigger`}
          className="launch-input"
          placeholder={kind?.trigger}
          value={draft.trigger}
          aria-invalid={(shown("trigger") && problems.trigger !== undefined) || undefined}
          spellCheck={false}
          autoComplete="off"
          onBlur={touch("trigger")}
          onChange={(event) => {
            change({ trigger: event.target.value });
          }}
        />
      </Field>
      <fieldset className="launch-field launch-handlers">
        <legend className="launch-label">Handlers</legend>
        <p className="launch-hint">In the order they run, middleware first.</p>
        {draft.handlers.map((handler, position) => {
          const problem = shown(`handler:${position}`) ? (problems.handlers[position] ?? null) : null;
          return (
            // Handlers have no identity besides their position while they're typed.
            <div key={position} className="launch-handler">
              <div className="launch-grow">
                <input
                  className="launch-input launch-mono"
                  aria-label={`Handler ${position + 1}`}
                  placeholder="src/cli.ts#runReport"
                  value={handler}
                  aria-invalid={problem !== null || undefined}
                  spellCheck={false}
                  autoComplete="off"
                  onBlur={touch(`handler:${position}`)}
                  onChange={(event) => {
                    setHandler(position, event.target.value);
                  }}
                />
                {problem === null ? null : <p className="launch-error">{problem}</p>}
              </div>
              <IconButton
                icon="back"
                className="launch-handler-earlier"
                label={`Run handler ${position + 1} earlier`}
                size={24}
                disabled={position === 0}
                onClick={() => {
                  const next = [...draft.handlers];
                  [next[position - 1], next[position]] = [next[position] ?? "", next[position - 1] ?? ""];
                  change({ handlers: next });
                }}
              />
              <IconButton
                icon="close"
                label={`Remove handler ${position + 1}`}
                size={24}
                disabled={draft.handlers.length === 1}
                onClick={() => {
                  change({ handlers: draft.handlers.filter((_, at) => at !== position) });
                }}
              />
            </div>
          );
        })}
        {shown("handlers") && problems.handlerList ? <p className="launch-error">{problems.handlerList}</p> : null}
        <Button
          size="sm"
          variant="ghost"
          icon="plus"
          onClick={() => {
            change({ handlers: [...draft.handlers, ""] });
          }}
        >
          Add handler
        </Button>
      </fieldset>
    </>
  );
}

/** The right column for a handler: the locator forms. */
export function LocatorForms() {
  return (
    <aside className="launch-preview" aria-label="Locator forms">
      <div className="launch-preview-head">
        <h3 className="launch-caps">Locator forms</h3>
      </div>
      <dl className="launch-parts">
        {LOCATOR_FORMS.map(({ form, selects }) => (
          <div key={form} className="launch-part">
            <dt>
              <code className="launch-chip">{form}</code>
            </dt>
            <dd>{selects}</dd>
          </div>
        ))}
      </dl>
      <p className="launch-note">Paths are relative to the project's folder. A name keeps working after edits; a line number doesn't.</p>
    </aside>
  );
}
