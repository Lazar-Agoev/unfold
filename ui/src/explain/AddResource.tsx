/**
 * Add resource…, from the canvas's menu: a small popover that asks
 * for the kind (table, queue, cache, storage, an outside service, or an actor), the name, and, for a
 * table, its columns. Columns are written by hand (name, type, and key mark; + Column adds one), or come
 * from the type that declares them (From a type…, a name search over types and classes): then they are
 * the type's fields, and only their key marks are the person's. A person's resource card needs no
 * citation. Adding saves at once as one undo step, at the place the menu was opened, and selects the
 * card; what the board refuses shows here and changes nothing.
 */

import { useRef, useState, type SyntheticEvent } from "react";
import { RESOURCE_ICONS } from "../canvas/resources/kinds";
import { columnTypeText, tableColumns } from "../canvas/resources/columns";
import { Button, Popover } from "../components";
import { CodeSearch } from "../entrances";
import { Icon } from "../icons";
import { useEngineQuery } from "../ipc";
import type { Column, ColumnKey, Key, NodeKind, ResourceKind } from "../ipc/generated";
import { useAuthor } from "../state/author";
import { addResourceCard, select, type PageTarget } from "../state/boards";
import "./explain.css";

/** The kinds, in the order the chips list them, with their words. */
const KINDS: readonly { readonly kind: ResourceKind; readonly word: string }[] = [
  { kind: "table", word: "Table" },
  { kind: "queue", word: "Queue" },
  { kind: "cache", word: "Cache" },
  { kind: "storage", word: "Storage" },
  { kind: "external", word: "Service" },
  { kind: "actor", word: "Actor" },
];

/** A column's key marks, as the mark picker offers them. */
const MARKS: readonly { readonly key: ColumnKey | ""; readonly text: string }[] = [
  { key: "", text: "—" },
  { key: "primary", text: "PK" },
  { key: "foreign", text: "FK" },
  { key: "unique", text: "unique" },
];

/** What From a type… finds: what declares fields. */
const TYPE_KINDS: readonly NodeKind[] = ["type", "class"];

/** A column being written: its name, type, and mark, as typed. */
interface Draft {
  readonly name: string;
  readonly type: string;
  readonly key: ColumnKey | "";
}

const BLANK: Draft = { name: "", type: "", key: "" };

/** A table's type, once picked: its key and name. */
interface PickedType {
  readonly key: Key;
  readonly name: string;
}

function MarkPicker({ name, value, onChange }: { readonly name: string; readonly value: ColumnKey | ""; readonly onChange: (key: ColumnKey | "") => void }) {
  return (
    <select
      className="resource-column-mark"
      aria-label={`Key mark of ${name || "the column"}`}
      value={value}
      onChange={(event) => {
        onChange(event.target.value as ColumnKey | "");
      }}
    >
      {MARKS.map((mark) => (
        <option key={mark.key} value={mark.key}>
          {mark.text}
        </option>
      ))}
    </select>
  );
}

/** The columns of a table backed by `type`: its fields, each with the mark the person gives it. */
function TypeColumns({ project, type, marks, onMark }: { readonly project: string; readonly type: Key; readonly marks: ReadonlyMap<string, ColumnKey>; readonly onMark: (name: string, key: ColumnKey | "") => void }) {
  const definition = useEngineQuery("type_definition", { project, key: type });
  const columns = tableColumns({ kind: "table", type }, definition.data);
  if (definition.isPending) {
    return <p className="resource-column-text type">Loading its fields…</p>;
  }
  return (
    <>
      {columns.map((column) => (
        <div key={column.name} className="resource-column">
          <span className="resource-column-text">{column.name}</span>
          <span className="resource-column-text type">{columnTypeText(column)}</span>
          <MarkPicker
            name={column.name}
            value={marks.get(column.name) ?? ""}
            onChange={(key) => {
              onMark(column.name, key);
            }}
          />
          <span />
        </div>
      ))}
    </>
  );
}

export interface AddResourceProps {
  readonly target: PageTarget;
  readonly anchor: HTMLElement;
  /** Where on the canvas the card goes. */
  readonly at: readonly [number, number];
  readonly onClose: () => void;
}

export function AddResource({ target, anchor, at, onClose }: AddResourceProps) {
  const author = useAuthor();
  const { project } = target.board;
  const nameField = useRef<HTMLInputElement>(null);
  const [kind, setKind] = useState<ResourceKind>("table");
  const [name, setName] = useState("");
  const [drafts, setDrafts] = useState<readonly Draft[]>([BLANK]);
  const [finding, setFinding] = useState(false);
  const [type, setType] = useState<PickedType | null>(null);
  const [marks, setMarks] = useState<ReadonlyMap<string, ColumnKey>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ready = name.trim() !== "" && author !== null && !busy;

  const draftAt = (index: number, change: Partial<Draft>) => {
    setDrafts((current) => current.map((draft, at) => (at === index ? { ...draft, ...change } : draft)));
    setError(null);
  };

  /** The table's columns as the file holds them: the marks laid on its type's fields, or the ones written. */
  const columns = (): Column[] => {
    if (type) {
      return [...marks].map(([column, key]) => ({ name: column, key }));
    }
    return drafts.flatMap((draft) =>
      draft.name.trim() === ""
        ? []
        : [{ name: draft.name.trim(), ...(draft.type.trim() !== "" ? { type: draft.type.trim() } : {}), ...(draft.key !== "" ? { key: draft.key } : {}) }],
    );
  };

  const submit = async (event: SyntheticEvent) => {
    event.preventDefault();
    if (!ready) {
      return;
    }
    setBusy(true);
    setError(null);
    const table = kind === "table";
    const outcome = await addResourceCard(target, {
      kind,
      name: name.trim(),
      ...(table && type ? { type: type.key } : {}),
      ...(table ? { columns: columns() } : {}),
      anchors: [],
      at,
      author,
    });
    if (outcome.ok) {
      select(target, { kind: "resource", resourceId: outcome.value });
      onClose();
    } else {
      setError(outcome.message);
      setBusy(false);
    }
  };

  return (
    <Popover open anchor={anchor} label="Add resource" side="bottom" align="start" width={360} placement="fixed" initialFocus={nameField} onClose={onClose}>
      <form className="explain-form" onSubmit={(event) => void submit(event)}>
        <p className="explain-title">Add resource</p>
        <div className="explain-fields">
          <div className="explain-field">
            <span className="explain-label">Kind</span>
            <div className="resource-kinds" role="group" aria-label="Kind">
              {KINDS.map((item) => (
                <button
                  key={item.kind}
                  type="button"
                  className="resource-kind"
                  aria-pressed={kind === item.kind}
                  onClick={() => {
                    setKind(item.kind);
                    setError(null);
                  }}
                >
                  <Icon name={RESOURCE_ICONS[item.kind]} size={12} />
                  {item.word}
                </button>
              ))}
            </div>
          </div>
          <label className="explain-field">
            <span className="explain-label">Name</span>
            <input
              ref={nameField}
              className="explain-input mono"
              value={name}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => {
                setName(event.target.value);
                setError(null);
              }}
            />
          </label>
          {kind === "table" ? (
            <div className="explain-field">
              <span className="explain-label">Columns</span>
              <div className="resource-columns" role="group" aria-label="Columns">
                {finding ? (
                  <div className="resource-type-search">
                    <CodeSearch
                      project={project}
                      kinds={TYPE_KINDS}
                      label="Type"
                      placeholder="Find the type that declares them"
                      onPick={(hit) => {
                        setType({ key: hit.key, name: hit.qualifiedName || hit.name });
                        setMarks(new Map());
                        setFinding(false);
                      }}
                    />
                  </div>
                ) : null}
                {type ? (
                  <TypeColumns
                    project={project}
                    type={type.key}
                    marks={marks}
                    onMark={(column, key) => {
                      setMarks((current) => {
                        const next = new Map(current);
                        if (key === "") {
                          next.delete(column);
                        } else {
                          next.set(column, key);
                        }
                        return next;
                      });
                    }}
                  />
                ) : (
                  drafts.map((draft, index) => (
                    <div key={index} className="resource-column">
                      <input
                        className="resource-column-input"
                        aria-label="Column name"
                        value={draft.name}
                        placeholder="name"
                        spellCheck={false}
                        autoComplete="off"
                        onChange={(event) => {
                          draftAt(index, { name: event.target.value });
                        }}
                      />
                      <input
                        className="resource-column-input"
                        aria-label="Column type"
                        value={draft.type}
                        placeholder="type"
                        spellCheck={false}
                        autoComplete="off"
                        onChange={(event) => {
                          draftAt(index, { type: event.target.value });
                        }}
                      />
                      <MarkPicker
                        name={draft.name}
                        value={draft.key}
                        onChange={(key) => {
                          draftAt(index, { key });
                        }}
                      />
                      <button
                        type="button"
                        className="resource-column-remove"
                        aria-label="Remove column"
                        onClick={() => {
                          setDrafts((current) => current.filter((_, at) => at !== index));
                        }}
                      >
                        <Icon name="close" size={10} />
                      </button>
                    </div>
                  ))
                )}
                <div className="resource-columns-foot">
                  {type ? (
                    <span className="resource-column-text type">{`From ${type.name}`}</span>
                  ) : (
                    <button
                      type="button"
                      className="resource-add-column"
                      onClick={() => {
                        setDrafts((current) => [...current, BLANK]);
                      }}
                    >
                      + Column
                    </button>
                  )}
                  <button
                    type="button"
                    className="resource-from-type"
                    onClick={() => {
                      if (type) {
                        setType(null);
                        setMarks(new Map());
                      } else {
                        setFinding((current) => !current);
                      }
                    }}
                  >
                    {type ? "Write them instead" : "From a type…"}
                  </button>
                </div>
              </div>
            </div>
          ) : null}
        </div>
        {error !== null ? <p className="explain-error">{error}</p> : null}
        <div className="explain-footer">
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
