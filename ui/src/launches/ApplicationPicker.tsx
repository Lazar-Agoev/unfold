/**
 * The launch's application: one of the project's `[[application]]`s, a new one (named here and
 * declared by the save), or none. It only groups launches; it never changes how imports resolve.
 */

import { IconButton } from "../components";
import type { ApplicationSpec } from "../ipc";
import { Icon } from "../icons";

const NONE = "";
const NEW = "\u0000new";

interface ApplicationPickerProps {
  readonly id: string;
  readonly declared: readonly ApplicationSpec[];
  readonly application: ApplicationSpec | null;
  /** The name typed for a new application, or null while one is chosen from the list. */
  readonly newName: string | null;
  readonly invalid: boolean;
  readonly onChange: (next: { readonly application: ApplicationSpec | null; readonly newName: string | null }) => void;
}

export function ApplicationPicker({ id, declared, application, newName, invalid, onChange }: ApplicationPickerProps) {
  if (newName !== null) {
    return (
      <div className="launch-new-application">
        <input
          id={id}
          className="launch-input"
          placeholder="New application's name"
          value={newName}
          aria-invalid={invalid || undefined}
          spellCheck={false}
          autoComplete="off"
          autoFocus
          onChange={(event) => {
            onChange({ application, newName: event.target.value });
          }}
        />
        <IconButton
          icon="close"
          size={24}
          label="Back to the list"
          onClick={() => {
            onChange({ application, newName: null });
          }}
        />
      </div>
    );
  }
  // A new application from a deploy file or a suggestion stays offered until it's saved.
  const pending = application !== null && !declared.some((candidate) => candidate.id === application.id) ? application : null;
  return (
    <div className="launch-select launch-select-native">
      <select
        id={id}
        value={application?.id ?? NONE}
        onChange={(event) => {
          const value = event.target.value;
          if (value === NEW) {
            onChange({ application, newName: "" });
            return;
          }
          const chosen = value === NONE ? null : (declared.find((candidate) => candidate.id === value) ?? pending);
          onChange({ application: chosen, newName: null });
        }}
      >
        <option value={NONE}>None</option>
        {declared.map((candidate) => (
          <option key={candidate.id} value={candidate.id}>
            {candidate.name}
          </option>
        ))}
        {pending ? <option value={pending.id}>{pending.name} (new)</option> : null}
        <option value={NEW}>New application…</option>
      </select>
      <Icon name="chevron-down-small" size={10} />
    </div>
  );
}
