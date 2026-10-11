/**
 * A type's definition, opened inline under the row that wrote it (docs/proposals/typed-lines.md §7):
 * its kind and name, `file:line`, its members with their own types (which open nested, up to four
 * deep), and "Peek source" (the peek sheet at its line) and "Used in N places" (a popover of the
 * places, each opening the peek sheet at its line). Which types are open is the caller's state.
 */

import { useState } from "react";
import { ListRow, Popover } from "../components";
import type { Key, ProjectId, TypeDefinition as TypeDefinitionModel, TypeKind, TypeMember } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { openPeek } from "../state/overlays";
import { TypeText } from "./TypeText";
import "./types.css";

/** Types open inside types up to this depth; deeper names read as plain names. */
export const MAX_TYPE_DEPTH = 4;

const KIND_WORDS: Readonly<Record<TypeKind, string>> = { interface: "interface", type_alias: "type", enum: "enum", class: "class" };

/** `src/cart.ts` and 4 → `cart.ts:4`. */
export function placeText(path: string, line: number): string {
  return `${path.slice(path.lastIndexOf("/") + 1)}:${line}`;
}

/** "Used in 6 places", "Used in 1 place". */
export function usesText(count: number): string {
  return `Used in ${count} ${count === 1 ? "place" : "places"}`;
}

/** A member's name column: two spaces of indent, then its name as written (`coupon?`, `[key]`, `()`). */
export function memberName(member: TypeMember): string {
  const name =
    member.name ?? (member.kind === "call" ? "()" : member.kind === "construct" ? "new ()" : member.kind === "index" ? "[key]" : "");
  return `  ${member.readonly ? "readonly " : ""}${name}${member.optional ? "?" : ""}`;
}

export interface TypeDefinitionProps {
  readonly project: ProjectId;
  readonly typeKey: Key;
  /** The row it opened under; its members' rows are `<row>/<index>`, its aliased type's `<row>/=`. */
  readonly row: string;
  /** 1 under a contract row, one more for each definition it sits in. */
  readonly depth: number;
  /** Each row's opened span (see `PageView.openTypes`). */
  readonly openTypes: Readonly<Record<string, number>>;
  readonly onToggle: (row: string, span: number) => void;
}

function UsedIn({ project, definition }: { readonly project: ProjectId; readonly definition: TypeDefinitionModel }) {
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
  // One written type: its row, its opened span, and the definition opened under it.
  const typeRow = (id: string, field: string, type: TypeDefinitionModel["aliased"], extra: string | null) => {
    const open = openTypes[id];
    const span = open !== undefined ? type?.spans[open] : undefined;
    const target = span?.style === "declared" ? span.target : undefined;
    return (
      <div key={id}>
        <div className="type-def-row">
          <span className="type-def-field">{field}</span>
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
        </div>
        {nested && target !== undefined ? (
          <div className="type-def-nested">
            <TypeDefinition project={project} typeKey={target} row={id} depth={depth + 1} openTypes={openTypes} onToggle={onToggle} />
          </div>
        ) : null}
      </div>
    );
  };
  return (
    <div className="type-definition" role="group" aria-label={`${KIND_WORDS[definition.kind]} ${definition.name}`}>
      <div className="type-def-row">
        <span className="type-def-kind">{KIND_WORDS[definition.kind]} </span>
        <span className="type-def-name">{definition.name}</span>
        <span className="type-def-place">{placeText(definition.path, definition.line)}</span>
      </div>
      {definition.aliased ? typeRow(`${row}/=`, "  =", definition.aliased, null) : null}
      {definition.members.map((member, index) =>
        typeRow(
          `${row}/${index}`,
          memberName(member),
          member.type,
          member.value !== undefined ? (member.type ? ` = ${member.value}` : member.value) : member.default ? " = …" : null,
        ),
      )}
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
