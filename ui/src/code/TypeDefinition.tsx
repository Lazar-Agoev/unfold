/**
 * A type's definition, opened inline under the row that wrote it (docs/reader.md):
 * its kind and name, `file:line`, its members with their own types (which open nested, up to four
 * deep; every type of one definition starts in the same column, after its widest name), the members it
 * inherits under a quiet "from <Type>" for each base (a base with no declaration says where it comes from
 * or why), and "Peek source" (the peek sheet at its line) and "Used in N places" (a popover of the
 * places, each opening the peek sheet at its line). Which types are open is the caller's state.
 */

import { Fragment, useState, type ReactNode } from "react";
import { ListRow, Popover } from "../components";
import type { InheritedGroup, Key, ProjectId, TypeDefinition as TypeDefinitionModel, TypeMember } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { openPeek } from "../state/overlays";
import { TypeText } from "./TypeText";
import { memberName, memberValue, typeKindWord } from "./typeKinds";
import "./types.css";

/** Types open inside types up to this depth; deeper names read as plain names. */
const MAX_TYPE_DEPTH = 4;

/** `src/cart.ts` and 4 → `cart.ts:4`. */
function placeText(path: string, line: number): string {
  return `${path.slice(path.lastIndexOf("/") + 1)}:${line}`;
}

/** "Used in 6 places", "Used in 1 place". */
function usesText(count: number): string {
  return `Used in ${count} ${count === 1 ? "place" : "places"}`;
}

/** The two spaces every member's name is indented by. */
const INDENT = "  ";

/** Where an inherited group's base comes from (an external one's module) or why it has no declaration. */
function baseNote(group: InheritedGroup): string | undefined {
  const span = group.from.spans[0];
  switch (span?.style) {
    case "external":
      return span.module ?? span.note;
    case "unresolved":
      return span.note;
    default:
      return undefined;
  }
}

/** The sub-heading over the members inherited from one base: `from OrderDraft`. */
export function FromBase({ group }: { readonly group: InheritedGroup }) {
  const note = baseNote(group);
  return (
    <div className="type-def-from">
      from <TypeText type={group.from} />
      {note !== undefined ? <span className="type-def-from-note">{` · ${note}`}</span> : null}
    </div>
  );
}

export interface TypeDefinitionProps {
  readonly project: ProjectId;
  readonly typeKey: Key;
  /**
   * The row it opened under; its members' rows are `<row>/<index>`, its aliased type's `<row>/=`, and
   * the members inherited from its `g`th base `<row>/from<g>/<index>`.
   */
  readonly row: string;
  /** 1 under a contract row, one more for each definition it sits in. */
  readonly depth: number;
  /** Each row's opened span (see `PageView.openTypes`). */
  readonly openTypes: Readonly<Record<string, number>>;
  readonly onToggle: (row: string, span: number) => void;
}

/** "Used in N places": a popover of the places the type is written, each opening the peek sheet at its line. */
export function UsedIn({ project, definition }: { readonly project: ProjectId; readonly definition: TypeDefinitionModel }) {
  const [anchor, setAnchor] = useState<HTMLButtonElement | null>(null);
  const uses = useEngineQuery("type_uses", { project, key: definition.key }, { enabled: anchor !== null });
  const close = () => {
    setAnchor(null);
  };
  return (
    <>
      <button
        type="button"
        className="type-def-action"
        aria-haspopup="dialog"
        aria-expanded={anchor !== null}
        disabled={definition.uses === 0}
        onClick={(event) => {
          event.stopPropagation();
          setAnchor(anchor ? null : event.currentTarget);
        }}
      >
        {usesText(definition.uses)}
      </button>
      <Popover open={anchor !== null} anchor={anchor} onClose={close} label={`Where ${definition.name} is used`} width={320} className="type-uses">
        {uses.data ? (
          <>
            <div role="list">
              {uses.data.places.map((place) => (
                <ListRow
                  key={`${place.owner}:${place.path}:${place.line}`}
                  role="listitem"
                  tabIndex={0}
                  className="type-uses-row"
                  label={<span className="type-uses-label">{place.label}</span>}
                  trailing={<span className="type-uses-place">{placeText(place.path, place.line)}</span>}
                  onClick={() => {
                    close();
                    openPeek({ project, path: place.path, line: place.line });
                  }}
                />
              ))}
            </div>
            {uses.data.total > uses.data.places.length ? (
              <p className="type-uses-more">
                The first {uses.data.places.length} of {uses.data.total}
              </p>
            ) : null}
          </>
        ) : (
          <p className="type-uses-more">{uses.isError ? uses.error.message : "Looking…"}</p>
        )}
      </Popover>
    </>
  );
}

export function TypeDefinition({ project, typeKey, row, depth, openTypes, onToggle }: TypeDefinitionProps) {
  const query = useEngineQuery("type_definition", { project, key: typeKey });
  const definition = query.data;
  if (!definition) {
    return <div className="type-definition note">{query.isError ? "This type isn't in the graph." : "Reading…"}</div>;
  }
  const nested = depth < MAX_TYPE_DEPTH;
  // One written type: its name and type cells in the members' grid, and the definition opened under it.
  const typeRow = (id: string, field: ReactNode, type: TypeDefinitionModel["aliased"], extra: string | null) => {
    const open = openTypes[id];
    const span = open !== undefined ? type?.spans[open] : undefined;
    const target = span?.style === "declared" ? span.target : undefined;
    return (
      <Fragment key={id}>
        <span className="type-def-field">{field}</span>
        <span className="type-def-type">
          {type ? (
            <TypeText
              type={type}
              open={open ?? null}
              {...(nested
                ? {
                    onToggle: (index: number) => {
                      onToggle(id, index);
                    },
                  }
                : {})}
            />
          ) : null}
          {extra !== null ? <span className="type-def-value">{extra}</span> : null}
        </span>
        {nested && target !== undefined ? (
          <div className="type-def-nested">
            <TypeDefinition project={project} typeKey={target} row={id} depth={depth + 1} openTypes={openTypes} onToggle={onToggle} />
          </div>
        ) : null}
      </Fragment>
    );
  };
  // A member's row: its name (with `readonly` drawn apart), its type, and its value or default.
  const memberRow = (id: string, member: TypeMember) =>
    typeRow(
      id,
      <>
        {INDENT}
        {member.readonly ? <span className="type-def-readonly">readonly </span> : null}
        {memberName(member)}
      </>,
      member.type,
      memberValue(member),
    );
  const rows = definition.aliased !== undefined || definition.members.length > 0 || definition.inherited.length > 0;
  return (
    <div className="type-definition" role="group" aria-label={`${typeKindWord(definition.kind)} ${definition.name}`}>
      <div className="type-def-row">
        <span className="type-def-kind">{typeKindWord(definition.kind)} </span>
        <span className="type-def-name">{definition.name}</span>
        <span className="type-def-place">{placeText(definition.path, definition.line)}</span>
      </div>
      {rows ? (
        // One grid for all the rows, inherited ones included, so every type starts in the same column.
        <div className="type-def-members">
          {definition.aliased ? typeRow(`${row}/=`, `${INDENT}=`, definition.aliased, null) : null}
          {definition.members.map((member, index) => memberRow(`${row}/${index}`, member))}
          {definition.inherited.map((group, base) => (
            <Fragment key={`from${base}`}>
              <FromBase group={group} />
              {group.members.map((member, index) => memberRow(`${row}/from${base}/${index}`, member))}
            </Fragment>
          ))}
        </div>
      ) : null}
      <div className="type-def-actions">
        <button
          type="button"
          className="type-def-action"
          onClick={(event) => {
            event.stopPropagation();
            openPeek({ project, path: definition.path, line: definition.line });
          }}
        >
          Peek source
        </button>
        <UsedIn project={project} definition={definition} />
      </div>
    </div>
  );
}
