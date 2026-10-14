/**
 * Add as entrance: a small popover that asks for the app
 * (one of the file's, or a new name), the entrance's name, and `on`, then writes one line into that app
 * with `add_entrance`, which creates the app when it is new and refuses a name whose key the app has
 * (the refusal shows in the popover). From a card, the card's key is the `code`, shown in the footer.
 * From Home there is no card, so a Code field finds it by name (`CodeSearch`); typing there again
 * drops the pick.
 */

import { useRef, useState, type ComponentProps, type SyntheticEvent } from "react";
import { Button, Popover } from "../components";
import { Icon } from "../icons";
import { toUnfoldError, useEngineQuery } from "../ipc";
import { call } from "../ipc/commands";
import type { Key, ProjectId } from "../ipc/generated";
import { CodeSearch } from "./CodeSearch";
import "./entrances.css";

/** The `<select>` value that switches the App field to a new name. */
const NEW_APP = "\u0000new";

export interface AddEntranceProps {
  readonly project: ProjectId;
  readonly anchor: HTMLElement;
  /** The card's key; without one, the Code field finds it. */
  readonly code?: Key;
  readonly align?: ComponentProps<typeof Popover>["align"];
  readonly side?: ComponentProps<typeof Popover>["side"];
  readonly onClose: () => void;
  /** After the line is written, with the app's name and the entrance's. */
  readonly onAdded?: (app: string, name: string) => void;
}

export function AddEntrance({ project, anchor, code: cardCode, align = "start", side = "bottom", onClose, onAdded }: AddEntranceProps) {
  const entrances = useEngineQuery("entrances", { project });
  const apps = (entrances.data?.apps ?? []).flatMap((app) => (app.name !== undefined && app.problem === undefined ? [app.name] : []));
  const nameField = useRef<HTMLInputElement>(null);
  // The app picked from the file's (the first until another is picked), or the new name typed.
  const [picked, setPicked] = useState<string | null>(null);
  const [typing, setTyping] = useState(false);
  const [newApp, setNewApp] = useState("");
  const [name, setName] = useState("");
  const [on, setOn] = useState("");
  const [found, setFound] = useState<Key | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Until the file's apps are known, the App field waits for them.
  const typingApp = typing || (!entrances.isPending && apps.length === 0);
  const app = (typingApp ? newApp : (picked ?? apps[0] ?? "")).trim();
  const code = cardCode ?? found;
  const ready = app !== "" && name.trim() !== "" && code !== null && !busy;

  const submit = async (event: SyntheticEvent) => {
    event.preventDefault();
    if (!ready) {
      return;
    }
    setBusy(true);
    setError(null);
    const entrance = { name: name.trim(), code: [code], ...(on.trim() === "" ? {} : { on: on.trim() }) };
    try {
      await call("update_project_config", { project, edits: [{ op: "add_entrance", app, entrance }] });
      onClose();
      onAdded?.(app, entrance.name);
    } catch (cause) {
      setError(toUnfoldError(cause).message);
      setBusy(false);
    }
  };

  return (
    <Popover open anchor={anchor} label="Add as entrance" align={align} side={side} width={340} initialFocus={nameField} onClose={onClose}>
      <form className="add-entrance" onSubmit={(event) => void submit(event)}>
        <p className="add-entrance-title">Add as entrance</p>
        <div className="add-entrance-fields">
          <label className="add-entrance-field">
            <span>App</span>
            {typingApp ? (
              <input
                className="add-entrance-input"
                value={newApp}
                placeholder="New app"
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => {
                  setNewApp(event.target.value);
                }}
              />
            ) : (
              <span className="add-entrance-select">
                <select
                  className="add-entrance-input"
                  disabled={entrances.isPending}
                  value={picked ?? apps[0]}
                  onChange={(event) => {
                    if (event.target.value === NEW_APP) {
                      setTyping(true);
                    } else {
                      setPicked(event.target.value);
                    }
                  }}
                >
                  {apps.map((item) => (
                    <option key={item} value={item}>
                      {item}
                    </option>
                  ))}
                  <option value={NEW_APP}>New app…</option>
                </select>
                <Icon name="chevron-down-small" size={10} />
              </span>
            )}
          </label>
          <label className="add-entrance-field">
            <span>Name</span>
            <input
              ref={nameField}
              className="add-entrance-input"
              value={name}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => {
                setName(event.target.value);
                setError(null);
              }}
            />
          </label>
          <label className="add-entrance-field">
            <span>On</span>
            <input
              className="add-entrance-input mono"
              value={on}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => {
                setOn(event.target.value);
              }}
            />
          </label>
          {cardCode === undefined ? (
            <div className="add-entrance-field">
              <span>Code</span>
              <CodeSearch
                project={project}
                onPick={(hit) => {
                  setFound(hit.key);
                }}
                onEdit={() => {
                  setFound(null);
                }}
              />
            </div>
          ) : null}
        </div>
        {error !== null ? <p className="add-entrance-error">{error}</p> : null}
        <div className="add-entrance-footer">
          <code title={code ?? undefined}>{code}</code>
          <Button variant="secondary" size="sm" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" type="submit" disabled={!ready}>
            Add
          </Button>
        </div>
      </form>
    </Popover>
  );
}
