/**
 * A type card (design §4.2): a type declaration or a class as its fields, in Diagram view (and in Flow,
 * for a type declaration, which has no paths). The header shows the kind (`interface`, `type`, `enum`,
 * `class`), the name, where it is, and the card's own switch; the review banner and the description
 * follow. Each field is its name as written (`readonly` drawn apart, `?`), its written type, and its
 * default (` = …`) or an enum member's value; a type alias that isn't an object shows its aliased type;
 * the fields it inherits follow under "from <Type>" for each base. A declared type opens its definition
 * in place, as in a line's contract. A card of 12 fields or more lists the first 8 and "N more".
 *
 * Each field with a type arrow has its port at its row's right edge (a field behind "N more" at that
 * row); calls from a class leave from the header's edge. Folded, it shows only its header.
 */

import { useQuery } from "@tanstack/react-query";
import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { Monogram } from "../../components";
import { FromBase, TypeDefinition } from "../../code/TypeDefinition";
import { TypeText } from "../../code/TypeText";
import { keyTypeWord, memberName, memberValue, typeKindWord } from "../../code/typeKinds";
import type { Card, ProjectId, TypeDefinition as TypeDefinitionModel } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { keyDisplayName, toggledOpenTypes, type PageTarget } from "../../state/boards";
import { fieldPort, fieldRows, type FieldRow } from "../arrows";
import { cardQuery, isMissing } from "../cardData";
import { ProjectScope } from "../ProjectScope";
import { useMeasure } from "../measure";
import { fieldTexts, listedFields, nodeLocation, typeCardWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { CardHandles, CardNote, DescriptionStrip, HeaderPorts, ViewSwitch, nodeHeaderPorts, renderPorts, toggleCardFolded } from "./parts";
import { ReviewBanner } from "./ReviewBanner";
import { clickCard } from "./select";

export interface TypeCardProps {
  readonly target: PageTarget;
  /** The project the card resolves to. */
  readonly project: ProjectId;
  readonly card: Card & { readonly ref: { readonly block: string } };
  /** Diagram, or Flow for a type declaration. */
  readonly view: "diagram" | "flow";
  readonly ports: readonly string[];
  readonly selected: boolean;
}

interface FieldsProps {
  readonly project: ProjectId;
  readonly definition: TypeDefinitionModel;
  readonly ports: ReadonlySet<string>;
  readonly openTypes: Readonly<Record<string, number>>;
  readonly onToggle: (row: string, span: number) => void;
  /** Whether every field is listed (after "N more"). */
  readonly all: boolean;
  readonly onShowAll: () => void;
}

/** One field: its name, its written type (a declared name opens its definition under the row), its value, and its port. */
function FieldRowView({ project, field, ports, openTypes, onToggle }: Omit<FieldsProps, "definition" | "all" | "onShowAll"> & { readonly field: FieldRow }) {
  const { row, member, type } = field;
  const open = openTypes[row];
  const span = open !== undefined ? type?.spans[open] : undefined;
  const opened = span?.style === "declared" ? span.target : undefined;
  const port = fieldPort(row);
  const value = member ? memberValue(member) : null;
  return (
    <>
      <span className="tcard-name">
        {member?.readonly ? <span className="type-def-readonly">readonly </span> : null}
        {member ? memberName(member) : "="}
      </span>
      <span className="tcard-type">
        {type ? (
          <TypeText
            type={type}
            open={open ?? null}
            onToggle={(index) => {
              onToggle(row, index);
            }}
          />
        ) : null}
        {value !== null ? <span className="type-def-value">{value}</span> : null}
        {ports.has(port) ? renderPorts([port]) : null}
      </span>
      {opened !== undefined ? (
        <div className="tcard-opened">
          <TypeDefinition project={project} typeKey={opened} row={row} depth={1} openTypes={openTypes} onToggle={onToggle} />
        </div>
      ) : null}
    </>
  );
}

function Fields({ project, definition, ports, openTypes, onToggle, all, onShowAll }: FieldsProps) {
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
        return field ? <FieldRowView key={field.row} project={project} field={field} ports={ports} openTypes={openTypes} onToggle={onToggle} /> : null;
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

export function TypeCard({ target, project, card, view, ports, selected }: TypeCardProps) {
  const key = card.ref.block;
  const query = useQuery(cardQuery(project, key));
  const model = query.data?.type === "block" ? query.data : undefined;
  const typeQuery = useEngineQuery("type_definition", { project, key });
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const folded = card.folded === true;
  const [openTypes, setOpenTypes] = useState<Readonly<Record<string, number>>>({});
  const [all, setAll] = useState(false);
  const header = model?.header;
  const source = useMemo(
    () =>
      header
        ? { path: header.path, line: header.startLine, definition: { path: header.path, kind: header.kind, name: header.name, qualifiedName: header.qualifiedName, line: header.startLine } }
        : null,
    [header],
  );
  const menu = useCardMenu(target, project, card.id, folded, true, source);
  const definition = typeQuery.data;
  const kind = definition ? typeKindWord(definition.kind) : keyTypeWord(key);
  const name = header?.qualifiedName ?? definition?.name ?? keyDisplayName(key);
  const location = header ? nodeLocation(header.path, header.startLine) : definition ? nodeLocation(definition.path, definition.line) : "";
  const foreign = card.ref.project;
  const width = useMemo(
    () => typeCardWidth({ mark: kind, name, location, monogram: foreign !== undefined }, definition ? fieldTexts(definition) : [], measure),
    [kind, name, location, foreign, definition, measure],
  );
  const portSet = useMemo(() => new Set(ports), [ports]);

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, folded, width, definition, openTypes, all]);

  const onToggle = (row: string, span: number) => {
    setOpenTypes((current) => toggledOpenTypes(current, row, span));
  };
  const missing = (query.isError && isMissing(query.error)) || (typeQuery.isError && isMissing(typeQuery.error));
  let body: ReactNode;
  if (definition) {
    body = <Fields project={project} definition={definition} ports={portSet} openTypes={openTypes} onToggle={onToggle} all={all} onShowAll={() => { setAll(true); }} />;
  } else if (typeQuery.isError || query.isError) {
    body = <CardNote warn>{missing ? "This type is missing from the graph." : (typeQuery.error ?? query.error)?.message}</CardNote>;
  } else {
    body = <CardNote>Loading…</CardNote>;
  }
  return (
    <article
      className={`board-card type-card${selected ? " selected" : ""}${folded ? " folded" : ""}${missing ? " missing" : ""}`}
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
      <header
        className="card-header card-drag"
        onDoubleClick={() => {
          toggleCardFolded(target, card.id, folded);
        }}
      >
        {foreign !== undefined ? <Monogram name={foreign} size={20} /> : null}
        <span className="tcard-kind">{kind}</span>
        <h3 className="card-title" title={name}>
          {name}
        </h3>
        <span className="card-location" title={location}>
          {location}
        </span>
        {!folded && model ? <ViewSwitch target={target} cardId={card.id} view={view} name={name} /> : null}
        <HeaderPorts ids={nodeHeaderPorts(ports, folded)} />
      </header>
      <ReviewBanner target={target} project={project} cardId={card.id} />
      {folded ? null : (
        <>
          <DescriptionStrip target={target} cardId={card.id} description={card.description} />
          <div className="card-body nodrag nopan">
            <ProjectScope.Provider value={project}>{body}</ProjectScope.Provider>
          </div>
        </>
      )}
      {menu.element}
    </article>
  );
}
