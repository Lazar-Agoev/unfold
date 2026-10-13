/**
 * A type card (design §4.2): a type declaration as its fields in either lens (it has no expansion
 * chevron), or a compact class in Diagram, whose header starts with the chevron that expands it to its
 * Flow (a class shows its Code in the Code lens, `cardBody`). The header shows the kind (`interface`,
 * `type`, `enum`, `class`), the name, and where it is; the review banner and the description follow.
 * Each field is its name as written (`readonly` drawn apart, `?`), its written type, and its default
 * (` = …`) or an enum member's value;
 * a type alias that isn't an object shows its aliased type; the fields it inherits follow under
 * "from <Type>" for each base. A card of 12 fields or more lists the first 8 and "N more".
 *
 * Each field with a type arrow has its port at its row's right edge (a field behind "N more" at that
 * row); calls from a class leave from the header's edge.
 */

import { useQuery } from "@tanstack/react-query";
import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { TypeText } from "../../code/TypeText";
import { fieldsOnly, keyTypeWord, memberName, memberValue, typeKindWord } from "../../code/typeKinds";
import type { Card, InheritedGroup, ProjectId, TypeDefinition } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { keyDisplayName, type PageTarget } from "../../state/boards";
import { fieldPort, fieldRows, type FieldRow } from "../arrows";
import { cardQuery, isMissing } from "../cardData";
import { useMeasure } from "../measure";
import { fieldTexts, listedFields, nodeLocation, typeCardWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { CardHandles, CardNote, DescriptionStrip, ExpandToggle, HeaderPorts, nodeHeaderPorts, renderPorts } from "./parts";
import { ReviewBanner } from "./ReviewBanner";
import { clickCard } from "./select";

export interface TypeCardProps {
  readonly target: PageTarget;
  /** The project the card resolves to. */
  readonly project: ProjectId;
  readonly card: Card & { readonly ref: { readonly block: string } };
  readonly ports: readonly string[];
  readonly selected: boolean;
}

interface FieldsProps {
  readonly definition: TypeDefinition;
  readonly ports: ReadonlySet<string>;
  /** Whether every field is listed (after "N more"). */
  readonly all: boolean;
  readonly onShowAll: () => void;
}

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
function FromBase({ group }: { readonly group: InheritedGroup }) {
  const note = baseNote(group);
  return (
    <div className="type-def-from">
      from <TypeText type={group.from} />
      {note !== undefined ? <span className="type-def-from-note">{` · ${note}`}</span> : null}
    </div>
  );
}

/** One field: its name, its written type, its value, and its port. */
function FieldRowView({ field, ports }: { readonly field: FieldRow; readonly ports: ReadonlySet<string> }) {
  const { row, member, type } = field;
  const port = fieldPort(row);
  const value = member ? memberValue(member) : null;
  return (
    <>
      <span className="tcard-name">
        {member?.readonly ? <span className="type-def-readonly">readonly </span> : null}
        {member ? memberName(member) : "="}
      </span>
      <span className="tcard-type">
        {type ? <TypeText type={type} /> : null}
        {value !== null ? <span className="type-def-value">{value}</span> : null}
        {ports.has(port) ? renderPorts([port]) : null}
      </span>
    </>
  );
}

function Fields({ definition, ports, all, onShowAll }: FieldsProps) {
  const rows = fieldRows(definition);
  const { entries, hidden } = listedFields(definition, all);
  if (entries.length === 0) {
    return <p className="tcard-empty">No fields written.</p>;
  }
  // The ports of the fields "N more" hides sit on its row.
  const hiddenPorts = rows.slice(rows.length - hidden).map((field) => fieldPort(field.row)).filter((port) => ports.has(port));
  return (
    <div className="tcard-fields">
      {entries.map((entry) => {
        if (entry.kind === "heading") {
          const group = definition.inherited[entry.base];
          return group ? <FromBase key={`from${entry.base}`} group={group} /> : null;
        }
        const field = rows[entry.index];
        return field ? <FieldRowView key={field.row} field={field} ports={ports} /> : null;
      })}
      {hidden > 0 ? (
        <button
          type="button"
          className="tcard-more"
          onClick={(event) => {
            event.stopPropagation();
            onShowAll();
          }}
        >
          {hidden} more
          {renderPorts(hiddenPorts)}
        </button>
      ) : null}
    </div>
  );
}

export function TypeCard({ target, project, card, ports, selected }: TypeCardProps) {
  const key = card.ref.block;
  const query = useQuery(cardQuery(project, key));
  const model = query.data?.type === "block" ? query.data : undefined;
  const typeQuery = useEngineQuery("type_definition", { project, key });
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const [all, setAll] = useState(false);
  const header = model?.header;
  const menu = useCardMenu(target, card.id, true);
  const definition = typeQuery.data;
  const kind = definition ? typeKindWord(definition.kind) : keyTypeWord(key);
  const name = header?.qualifiedName ?? definition?.name ?? keyDisplayName(key);
  const location = header ? nodeLocation(header.path, header.startLine) : definition ? nodeLocation(definition.path, definition.line) : "";
  // A class expands to its detail; a type declaration has only its fields.
  const expandable = !fieldsOnly(key);
  const width = useMemo(
    () => typeCardWidth({ mark: kind, name, location, chevron: expandable }, definition ? fieldTexts(definition) : [], measure),
    [kind, name, location, expandable, definition, measure],
  );
  const portSet = useMemo(() => new Set(ports), [ports]);

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, width, definition, all]);

  const missing = (query.isError && isMissing(query.error)) || (typeQuery.isError && isMissing(typeQuery.error));
  let body: ReactNode;
  if (definition) {
    body = <Fields definition={definition} ports={portSet} all={all} onShowAll={() => { setAll(true); }} />;
  } else if (typeQuery.isError || query.isError) {
    body = <CardNote warn>{missing ? "This type is missing from the graph." : (typeQuery.error ?? query.error)?.message}</CardNote>;
  } else {
    body = <CardNote>Loading…</CardNote>;
  }
  return (
    <article
      className={`board-card type-card${selected ? " selected" : ""}${missing ? " missing" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={typeQuery.isPending ? "true" : undefined}
      style={{ width }}
      aria-label={`${kind} ${name}`}
      onClick={() => {
        clickCard(target, card.id, null);
      }}
      onContextMenu={menu.onContextMenu}
    >
      <CardHandles />
      <header className="card-header card-drag">
        {expandable ? <ExpandToggle target={target} cardId={card.id} expanded={false} name={name} /> : null}
        <span className="tcard-kind">{kind}</span>
        <h3 className="card-title" title={name}>
          {name}
        </h3>
        <span className="card-location" title={location}>
          {location}
        </span>
        <HeaderPorts ids={nodeHeaderPorts(ports)} />
      </header>
      <ReviewBanner target={target} project={project} cardId={card.id} />
      <DescriptionStrip target={target} cardId={card.id} description={card.description} />
      <div className="card-body nodrag nopan">{body}</div>
      {menu.element}
    </article>
  );
}
