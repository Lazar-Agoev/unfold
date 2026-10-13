/**
 * A resource card on the canvas (design §4.5–4.6): what the code talks to that isn't a function, drawn
 * in the authored style and the same in every view. Its header shows its kind's glyph and word, its
 * name, and the agent mark when an agent drew it; the review banner and its whole description follow.
 * A table lists its columns: each name (`?` when optional), its written type (a type-backed table's
 * types are the fields' own, whose declared names open their definitions in place), and its marks
 * (`PK`, `FK → customers.id`, `unique`); up to 12, then "N more". A relationship (`relates`) leaves
 * and enters at a column's row; other lines at the card's edges.
 *
 * A click selects it (or ends a link being drawn there); its menu has Describe…, Copy link, and Remove
 * resource card; double-clicking its description edits it in place.
 */

import { Handle, Position, useUpdateNodeInternals, type Node, type NodeProps } from "@xyflow/react";
import { Fragment, memo, useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { AgentMark, copyLink } from "../../agent";
import { TypeDefinition } from "../../code/TypeDefinition";
import { TypeText } from "../../code/TypeText";
import { authorOf } from "../../code/text";
import { useContextMenu } from "../../components";
import { Icon } from "../../icons";
import type { ProjectId, Resource } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { describeResource, findPage, findResource, removeItem, select, toggledOpenTypes, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import { showInSelected } from "../cards/cardMenu";
import { CardHandles, DescriptionStrip } from "../cards/parts";
import { ClaimReviewBanner } from "../cards/ReviewBanner";
import { clickResource } from "../cards/select";
import { useMeasure } from "../measure";
import { resourceCardWidth } from "../sizing";
import { columnHandle, listedColumns, markText, tableColumns, type TableColumn } from "./columns";
import { RESOURCE_ICONS, RESOURCE_WORDS } from "./kinds";
import "./resources.css";

interface ResourceNodeData extends Record<string, unknown> {
  readonly target: PageTarget;
  /** The columns a relationship leaves or enters by. */
  readonly columns: readonly string[];
}

export type ResourceNodeType = Node<ResourceNodeData, "resource">;

/** The focus target of a resource card's description field in Selected. */
export function resourceDescriptionField(resourceId: string): string {
  return `selected.resource:${resourceId}`;
}

/** A column's relationship handles on one edge of the card: leaving and entering. */
function ColumnHandles({ name, side }: { readonly name: string; readonly side: "left" | "right" }) {
  const position = side === "left" ? Position.Left : Position.Right;
  return (
    <>
      <Handle id={columnHandle(name, "out", side)} type="source" position={position} isConnectable={false} className={`column-handle ${side}`} />
      <Handle id={columnHandle(name, "in", side)} type="target" position={position} isConnectable={false} className={`column-handle ${side}`} />
    </>
  );
}

interface ColumnsProps {
  readonly project: ProjectId;
  readonly columns: readonly TableColumn[];
  /** The columns with relationship handles. */
  readonly handles: ReadonlySet<string>;
}

/** A table's columns: name, written type, and marks; up to 12, then "N more" (which holds the handles of what it hides). */
function Columns({ project, columns, handles }: ColumnsProps) {
  const [all, setAll] = useState(false);
  const [openTypes, setOpenTypes] = useState<Readonly<Record<string, number>>>({});
  const { shown, hidden } = listedColumns(columns, all);
  if (columns.length === 0) {
    return <p className="rcard-empty">No columns listed.</p>;
  }
  const onToggle = (row: string, span: number) => {
    setOpenTypes((current) => toggledOpenTypes(current, row, span));
  };
  const hiddenHandles = columns.slice(shown.length).filter((column) => handles.has(column.name));
  return (
    <div className="rcard-columns">
      {shown.map((column, index) => {
        const row = String(index);
        const open = openTypes[row];
        const span = typeof column.type === "object" && open !== undefined ? column.type.spans[open] : undefined;
        const opened = span?.style === "declared" ? span.target : undefined;
        let type: ReactNode = null;
        if (typeof column.type === "string") {
          type = column.type;
        } else if (column.type) {
          type = (
            <TypeText
              type={column.type}
              open={open ?? null}
              onToggle={(spanIndex) => {
                onToggle(row, spanIndex);
              }}
            />
          );
        }
        return (
          <Fragment key={column.name}>
            <span className="rcard-name" title={column.from !== undefined ? `From ${column.from}` : undefined}>
              {column.name}
              {column.optional ? "?" : ""}
              {handles.has(column.name) ? <ColumnHandles name={column.name} side="left" /> : null}
            </span>
            <span className="rcard-type">{type}</span>
            <span className="rcard-mark">
              {markText(column)}
              {handles.has(column.name) ? <ColumnHandles name={column.name} side="right" /> : null}
            </span>
            {opened !== undefined ? (
              <div className="rcard-opened">
                <TypeDefinition project={project} typeKey={opened} row={row} depth={1} openTypes={openTypes} onToggle={onToggle} />
              </div>
            ) : null}
          </Fragment>
        );
      })}
      {hidden > 0 ? (
        <button
          type="button"
          className="rcard-more"
          onClick={(event) => {
            event.stopPropagation();
            setAll(true);
          }}
        >
          {hidden} more
          {hiddenHandles.map((column) => (
            <Fragment key={column.name}>
              <ColumnHandles name={column.name} side="left" />
              <ColumnHandles name={column.name} side="right" />
            </Fragment>
          ))}
        </button>
      ) : null}
    </div>
  );
}

/** A table's columns, from its type's definition (once loaded) or as it lists them, and whether its type is still loading. */
function useColumns(project: ProjectId, resource: Resource | undefined): { readonly columns: TableColumn[]; readonly loading: boolean } {
  const typeKey = resource?.kind === "table" ? resource.type : undefined;
  const wanted = typeKey !== undefined;
  const query = useEngineQuery("type_definition", { project, key: typeKey ?? "" }, { enabled: wanted });
  const columns = useMemo(() => (resource ? tableColumns(resource, query.data) : []), [resource, query.data]);
  return { columns, loading: wanted && query.isPending };
}

export const ResourceNode = memo(function ResourceNode({ id, data }: NodeProps<ResourceNodeType>) {
  const { target, columns: relates } = data;
  const resource = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findResource(page, id) : undefined;
  });
  const selected = usePageView(target, (view) => view.selection.kind === "resource" && view.selection.resourceId === id);
  const { project } = target.board;
  const { columns, loading } = useColumns(project, resource);
  const measure = useMeasure();
  const menu = useContextMenu("Resource card actions", 220);
  const updateNodeInternals = useUpdateNodeInternals();
  const handles = useMemo(() => new Set(relates), [relates]);
  const width = useMemo(
    () => (resource ? resourceCardWidth({ word: RESOURCE_WORDS[resource.kind], name: resource.name, agent: resource.author.startsWith("agent:"), columns }, measure) : 0),
    [resource, columns, measure],
  );
  const handlesKey = relates.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(id);
  }, [updateNodeInternals, id, handlesKey, columns, width]);
  if (!resource) {
    return null;
  }
  const writer = authorOf(resource.author);
  const word = RESOURCE_WORDS[resource.kind];
  const body: ReactNode = loading ? <p className="rcard-empty">Loading…</p> : <Columns project={project} columns={columns} handles={handles} />;
  return (
    <article
      className={`board-card resource-card kind-${resource.kind}${selected ? " selected" : ""}`}
      data-resource-id={id}
      style={{ width }}
      aria-label={`${word} ${resource.name}`}
      onClick={() => {
        clickResource(target, id);
      }}
      onContextMenu={(event) => {
        event.stopPropagation();
        select(target, { kind: "resource", resourceId: id });
        menu.open(event, [
          {
            id: "describe",
            label: "Describe…",
            onSelect: () => {
              showInSelected(resourceDescriptionField(id));
            },
          },
          { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, id) },
          { kind: "separator", id: "s1" },
          {
            id: "remove",
            label: "Remove resource card",
            danger: true,
            onSelect: () => {
              removeItem(target, id);
            },
          },
        ]);
      }}
    >
      <CardHandles />
      <header className="card-header resource-drag">
        <Icon name={RESOURCE_ICONS[resource.kind]} className="rcard-glyph" />
        <span className="rcard-kind">{word}</span>
        <h3 className="card-title" title={resource.name}>
          {resource.name}
        </h3>
        {writer.agent ? <AgentMark name={writer.name} /> : null}
      </header>
      <ClaimReviewBanner target={target} project={project} item={{ resource: id }} />
      <DescriptionStrip
        target={target}
        cardId={id}
        description={resource.description}
        wrap
        onSave={(text, author) => {
          describeResource(target, id, text, author);
        }}
      />
      {resource.kind === "table" ? <div className="card-body nodrag nopan">{body}</div> : null}
      {menu.element}
    </article>
  );
});
