/**
 * Project settings → Evidence files: the `[evidence] files` patterns, each with Remove, and a field
 * that adds one. Changes are staged; the dialog saves them with the rest.
 */

import { useState } from "react";
import { Button, IconButton, TextField } from "../components";
import { evidenceProblem } from "./projectSettings";

interface EvidenceSectionProps {
  readonly patterns: readonly string[];
  readonly onChange: (next: readonly string[]) => void;
}

export function EvidenceSection({ patterns, onChange }: EvidenceSectionProps) {
  const [text, setText] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const add = () => {
    const found = evidenceProblem(text, patterns);
    setProblem(found);
    if (found === null) {
      onChange([...patterns, text.trim()]);
      setText("");
    }
  };
  return (
    <>
      {patterns.length === 0 ? (
        <p className="settings-empty">None listed: boards cite code only.</p>
      ) : (
        <ul className="settings-list" aria-label="Evidence files">
          {patterns.map((pattern) => (
            <li key={pattern} className="settings-list-row">
              <code className="settings-evidence-pattern">{pattern}</code>
              <IconButton
                icon="close"
                label={`Remove ${pattern}`}
                size={24}
                onClick={() => {
                  onChange(patterns.filter((item) => item !== pattern));
                }}
              />
            </li>
          ))}
        </ul>
      )}
      <form
        className="settings-evidence-add"
        onSubmit={(event) => {
          event.preventDefault();
          add();
        }}
      >
        <TextField
          label="Evidence file"
          hideLabel
          className="settings-grow"
          placeholder="db/migrations/*.sql"
          value={text}
          error={problem}
          hint="A file, a folder, or a glob, relative to this folder."
          onChange={(event) => {
            setText(event.target.value);
            setProblem(null);
          }}
        />
        <Button variant="secondary" icon="plus" type="submit">
          Add
        </Button>
      </form>
    </>
  );
}
