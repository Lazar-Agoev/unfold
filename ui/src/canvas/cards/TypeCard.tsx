/**
 * A type card (design §4.2): a type declaration as its fields in every view (it has no chevron), or a
 * compact class, whose header starts with the chevron that expands it to its class-level code
 * (`BlockCard`). The header shows the kind (`interface`, `type`, `enum`, `class`), the name, and where
 * it is, from what the reader drew; the review banner and the description follow. The fields come from
 * the type's definition (`type_definition`).
 * Each field is its name as written (`readonly` drawn apart, `?`), its written type, and its default
 * (` = …`) or an enum member's value;
 * a type alias that isn't an object shows its aliased type; the fields it inherits follow under
 * "from <Type>" for each base. A card of 12 fields or more lists the first 8 and "N more".
 *
 * Each field with a type arrow has its port at its row's right edge (a field behind "N more" at that
 * row); calls from a class leave from the header's edge. Its header's `</>` opens its code in the code
 * panel, and a line of it hovered there lights the card. A field selects on a click, and it or its
 * `</>` opens that field's line (an inherited one in its base).
 */

import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { CodeButton } from "../../code/CodeButton";
import { viewCode } from "../../code/open";
import { TypeText } from "../../code/TypeText";
import { memberName, memberValue } from "../../code/typeKinds";
import type { Card, CardView, InheritedGroup, TypeDefinition } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { cardKey, clickItem, usePageView, type PageTarget, type Selection } from "../../state/boards";
import { isMissing } from "../../state/cards";
import { useCodeHover } from "../../state/codePanel";
import { NODE_PORT, fieldPort, fieldRows, type FieldRef, type FieldRow } from "../arrows";
import { useMeasure } from "../measure";
import { fieldTexts, headerText, listedFields, typeCardWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { CardHandles, CardMark, CardNote, DescriptionStrip, ExpandToggle, HeaderPorts, renderPorts } from "./parts";
import { ReviewBanner } from "./ReviewBanner";

export interface TypeCardProps {
  readonly target: PageTarget;
  readonly card: Card;
  /** What the reader drew for it: a type's or a class's header. */
  readonly drawn: Extract<CardView, { type: "definition" }>;
  readonly ports: readonly string[];
  readonly selected: boolean;
}

/** Selecting a field, and opening its line in the code panel. */
interface FieldActions {
  /** The selected field, when it is on this card. */
  readonly selected: FieldRef | null;
  readonly onSelect: (field: FieldRef) => void;
  readonly onView: (field: FieldRef) => void;
}

interface FieldsProps extends FieldActions {
  readonly definition: TypeDefinition;
  readonly ports: ReadonlySet<string>;
  /** Whether every field is listed (after "N more"). */
  readonly all: boolean;
  readonly onShowAll: () => void;
}

/** Where an inherited group's base comes from (a library's module) or why it has no declaration. */
function baseNote(group: InheritedGroup): string | undefined {
  const to = group.from.spans[0]?.to;
  switch (to?.outcome) {
    case "library":
      return to.module ?? to.note;
    case "unknown":
      return to.note;
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

/** One field: its name, its written type, its value, and its port; a member selects, with its `</>`. */
function FieldRowView({ field, ports, actions }: { readonly field: FieldRow; readonly ports: ReadonlySet<string>; readonly actions: FieldActions }) {
  const { row, member, type } = field;
  const port = fieldPort(row);
  const value = member ? memberValue(member) : null;
  const cells = (
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
  const ref = field.field;
  if (!ref) {
    return <div className="tcard-field">{cells}</div>;
  }
  const selected = actions.selected?.key === ref.key && actions.selected.index === ref.index;
  return (
    <div
      className={`tcard-field member${selected ? " selected" : ""}`}
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      onClick={(event) => {
        event.stopPropagation();
        actions.onSelect(ref);
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          event.stopPropagation();
          actions.onSelect(ref);
        }
      }}
    >
      {cells}
      <CodeButton
        className="tcard-code"
        onView={() => {
          actions.onView(ref);
        }}
      />
    </div>
  );
}

function Fields({ definition, ports, all, onShowAll, ...actions }: FieldsProps) {
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
        return field ? <FieldRowView key={field.row} field={field} ports={ports} actions={actions} /> : null;
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

export function TypeCard({ target, card, drawn, ports, selected }: TypeCardProps) {
  const key = cardKey(card);
  const { project } = target.board;
  const { header } = drawn.flow;
  const typeQuery = useEngineQuery("type_definition", { project, key });
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const [all, setAll] = useState(false);
  const menu = useCardMenu(target, card, header.kind === "class");
  const lit = useCodeHover()?.key === key;
  const selectedField = usePageView(target, ({ selection }) => (selection.kind === "field" && selection.cardId === card.id ? selection : null));
  const fieldSelection = (field: FieldRef): Selection => ({ kind: "field", cardId: card.id, ...field });
  const definition = typeQuery.data;
  // A class expands to its class-level code; a type declaration has only its fields.
  const expandable = header.kind === "class";
  const text = useMemo(() => headerText(header, expandable), [header, expandable]);
  const width = useMemo(() => typeCardWidth(text, definition ? fieldTexts(definition) : [], measure), [text, definition, measure]);
  const portSet = useMemo(() => new Set(ports), [ports]);

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, width, definition, all]);

  const missing = typeQuery.isError && isMissing(typeQuery.error);
  let body: ReactNode;
  if (definition) {
    body = (
      <Fields
        definition={definition}
        ports={portSet}
        all={all}
        onShowAll={() => {
          setAll(true);
        }}
        selected={selectedField}
        onSelect={(field) => {
          clickItem(target, card.id, fieldSelection(field));
        }}
        onView={(field) => {
          viewCode(target, fieldSelection(field), new Map([[card.id, drawn]]));
        }}
      />
    );
  } else if (typeQuery.isError) {
    body = <CardNote warn>{missing ? "This type is missing from the graph." : typeQuery.error.message}</CardNote>;
  } else {
    body = <CardNote>Loading…</CardNote>;
  }
  return (
    <article
      className={`board-card type-card${selected ? " selected" : ""}${lit ? " lit" : ""}${missing ? " missing" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={typeQuery.isPending ? "true" : undefined}
      style={{ width }}
      aria-label={`${text.mark.text} ${text.name}`}
      onClick={() => {
        clickItem(target, card.id, { kind: "card", cardId: card.id, key: null });
      }}
      onContextMenu={menu.onContextMenu}
    >
      <CardHandles />
      <header className="card-header card-drag">
        {expandable ? <ExpandToggle target={target} cardId={card.id} expanded={false} name={text.name} /> : null}
        <CardMark mark={text.mark} />
        <h3 className="card-title" title={text.name}>
          {text.name}
        </h3>
        <span className="card-location" title={`${header.path}:${header.line}`}>
          {text.location}
        </span>
        <CodeButton
          className="card-view-code"
          onView={() => {
            viewCode(target, { kind: "card", cardId: card.id, key: null }, new Map([[card.id, drawn]]));
          }}
        />
        <HeaderPorts ids={ports.filter((id) => id === NODE_PORT)} />
      </header>
      <ReviewBanner target={target} cardId={card.id} view={drawn} />
      <DescriptionStrip target={target} cardId={card.id} description={card.description} />
      <div className="card-body nodrag nopan">{body}</div>
      {menu.element}
    </article>
  );
}
