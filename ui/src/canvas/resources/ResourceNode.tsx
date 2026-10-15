/**
 * A resource card on the canvas: what the code talks to that isn't code, drawn the same in every view
 * with a blue outline, so it never reads as a code card. A table is an ERD entity: a tinted band with
 * its glyph and name, its description, then one row per column: its key in a fixed lane (`PK`, `FK`,
 * `UQ`), its name (`?` when optional), and its written type (a type-backed table's are the fields'
 * own) aligned right; up to 12, then "N more". A relationship (`relates`) leaves and enters at a
 * column's row. Every other kind is a glyph tile, its kind in small caps, its name, and its
 * description, and changes one thing about its shape: a queue adds a lane of messages and the type it
 * carries; storage is a stack; an outside service is dashed; an actor is a pill with its description
 * under it. Who added it shows in Selected.
 *
 * A click selects it (or ends a link being drawn there); its menu has Describe…, Draw link…, Copy link,
 * and Remove resource card; double-clicking its description edits it in place.
 */

import { Handle, Position, useUpdateNodeInternals, type Node, type NodeProps } from "@xyflow/react";
import { Fragment, memo, useLayoutEffect, useMemo, useState, type CSSProperties, type MouseEvent, type ReactNode } from "react";
import { copyLink } from "../../agent";
import { TypeText } from "../../code/TypeText";
import { useContextMenu } from "../../components";
import { Icon } from "../../icons";
import type { ColumnKey, Resource } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { clickItem, describeResource, findPage, findResource, removeItem, select, startLink, useOpenBoard, usePageView, type PageTarget } from "../../state/boards";
import { showInSelected } from "../cards/cardMenu";
import { CardHandles, DescriptionStrip } from "../cards/parts";
import { ClaimReviewBanner } from "../cards/ReviewBanner";
import { useMeasure } from "../measure";
import { resourceCardWidth, resourceNameFit } from "../sizing";
import { columnHandle, listedColumns, markText, type TableColumn } from "./columns";
import { RESOURCE_ICONS, RESOURCE_WORDS } from "./kinds";
import { useColumns } from "./useColumns";
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

/** Each key's word in a table's key lane. */
const KEY_WORDS: Readonly<Record<ColumnKey, string>> = { primary: "PK", foreign: "FK", unique: "UQ" };

interface ColumnsProps {
  readonly columns: readonly TableColumn[];
  /** The columns with relationship handles. */
  readonly handles: ReadonlySet<string>;
}

/** A table's rows: its key's lane, its name, and its written type; up to 12, then "N more" (which holds the handles of what it hides). */
function Columns({ columns, handles }: ColumnsProps) {
  const [all, setAll] = useState(false);
  const { shown, hidden } = listedColumns(columns, all);
  const hiddenHandles = columns.slice(shown.length).filter((column) => handles.has(column.name));
  return (
    <div className="rcard-columns">
      {shown.map((column) => {
        const type = typeof column.type === "string" ? column.type : column.type ? <TypeText type={column.type} /> : null;
        const mark = markText(column);
        const about = [mark, column.from !== undefined ? `From ${column.from}` : ""].filter((part) => part.length > 0).join(" · ");
        return (
          <div key={column.name} className={`rcard-row${column.key === "primary" ? " primary" : ""}`} title={about.length > 0 ? about : undefined}>
            <span className={`rcard-key${column.key !== undefined ? ` key-${column.key}` : ""}`}>{column.key !== undefined ? KEY_WORDS[column.key] : null}</span>
            <span className="rcard-name">
              {column.name}
              {column.optional ? "?" : ""}
            </span>
            <span className="rcard-type">{type}</span>
            {handles.has(column.name) ? (
              <>
                <ColumnHandles name={column.name} side="left" />
                <ColumnHandles name={column.name} side="right" />
              </>
            ) : null}
          </div>
        );
      })}
      {hidden > 0 ? (
        <button
          type="button"
          className="rcard-row rcard-more"
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

/** The type a queue carries: the first its page's queue connections name, by its definition's name. */
function useCarried(target: PageTarget, resourceId: string, kind: Resource["kind"]): string | undefined {
  const key = useOpenBoard(target.board, (board) => {
    const page = kind === "queue" ? findPage(board.document, target.page) : undefined;
    return page?.links.find((link) => link.kind === "queue" && link.carries !== undefined && (link.from === resourceId || link.to === resourceId))?.carries;
  });
  const query = useEngineQuery("type_definition", { project: target.board.project, key: key ?? "" }, { enabled: key !== undefined });
  return key !== undefined ? query.data?.name : undefined;
}

/** A queue's lane: three messages waiting, an arrow, and the type they carry. */
function QueueLane({ carries }: { readonly carries: string | undefined }) {
  return (
    <div className="rcard-lane" aria-label={carries !== undefined ? `Carries ${carries}` : undefined}>
      <span className="rcard-slot" />
      <span className="rcard-slot" />
      <span className="rcard-slot last" />
      <svg className="rcard-lane-arrow" width="16" height="10" viewBox="0 0 16 10" aria-hidden="true">
        <path d="M1 5H14M11 2L14 5L11 8" />
      </svg>
      {carries !== undefined ? <span className="rcard-carries">{carries}</span> : null}
    </div>
  );
}

/** A tile's or a pill's words: the kind in small caps over the name. */
function Words({ resource }: { readonly resource: Resource }) {
  return (
    <span className="rcard-words">
      <span className="rcard-kind">{RESOURCE_WORDS[resource.kind]}</span>
      <h3 className="card-title rcard-name-text" title={resource.name}>
        {resource.name}
      </h3>
    </span>
  );
}

export const ResourceNode = memo(function ResourceNode({ id, data }: NodeProps<ResourceNodeType>) {
  const { target, columns: relates } = data;
  const resource = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? findResource(page, id) : undefined;
  });
  const selected = usePageView(target, (view) => view.selection.kind === "resource" && view.selection.resourceId === id);
  const { project } = target.board;
  const { columns } = useColumns(project, resource);
  const carries = useCarried(target, id, resource?.kind ?? "table");
  const measure = useMeasure();
  const menu = useContextMenu("Resource card actions", 220);
  const updateNodeInternals = useUpdateNodeInternals();
  const handles = useMemo(() => new Set(relates), [relates]);
  const width = useMemo(
    () =>
      resource
        ? resourceCardWidth(
            {
              kind: resource.kind,
              name: resource.name,
              columns,
              ...(resource.description ? { description: resource.description } : {}),
              ...(carries !== undefined ? { carries } : {}),
            },
            measure,
          )
        : 0,
    [resource, columns, carries, measure],
  );
  const fit = useMemo(() => (resource ? resourceNameFit(resource, width, measure) : 1), [resource, width, measure]);
  const handlesKey = relates.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(id);
  }, [updateNodeInternals, id, handlesKey, columns, width]);
  if (!resource) {
    return null;
  }
  const word = RESOURCE_WORDS[resource.kind];
  const kind = resource.kind;
  const onClick = () => {
    clickItem(target, id, { kind: "resource", resourceId: id });
  };
  const onContextMenu = (event: MouseEvent) => {
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
      { id: "draw-link", label: "Draw link…", onSelect: () => { startLink(target, id); } },
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
  };
  const banner = <ClaimReviewBanner target={target} project={project} item={{ resource: id }} />;
  const description = (
    <DescriptionStrip
      target={target}
      cardId={id}
      description={resource.description}
      wrap
      onSave={(text, author) => {
        describeResource(target, id, text, author);
      }}
    />
  );
  const glyph = <Icon name={RESOURCE_ICONS[kind]} className="rcard-glyph" />;
  let body: ReactNode;
  if (kind === "actor") {
    // A person isn't a box: the pill takes the selection and the lines; the description sits under it.
    return (
      <article className="resource-actor" data-resource-id={id} style={{ width, "--name-fit": fit } as CSSProperties} aria-label={`${word} ${resource.name}`} onClick={onClick} onContextMenu={onContextMenu}>
        <div className={`board-card resource-card kind-actor${selected ? " selected" : ""}`}>
          <CardHandles />
          <header className="card-header resource-drag">
            <span className="rcard-tile">
              <Icon name={RESOURCE_ICONS[kind]} size={18} className="rcard-glyph" />
            </span>
            <Words resource={resource} />
          </header>
        </div>
        {banner}
        {description}
        {menu.element}
      </article>
    );
  }
  if (kind === "table") {
    body = (
      <>
        <header className="card-header resource-drag rcard-band">
          {glyph}
          <h3 className="card-title rcard-name-text" title={resource.name}>
            {resource.name}
          </h3>
        </header>
        {banner}
        {description}
        {columns.length > 0 ? (
          <div className="card-body nodrag nopan">
            <Columns columns={columns} handles={handles} />
          </div>
        ) : null}
      </>
    );
  } else {
    body = (
      <>
        <header className="card-header resource-drag">
          <span className="rcard-tile">{glyph}</span>
          <Words resource={resource} />
        </header>
        {banner}
        {description}
        {kind === "queue" ? <QueueLane carries={carries} /> : null}
      </>
    );
  }
  return (
    <article
      className={`board-card resource-card kind-${kind}${selected ? " selected" : ""}`}
      data-resource-id={id}
      style={{ width, "--name-fit": fit } as CSSProperties}
      aria-label={`${word} ${resource.name}`}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      <CardHandles />
      {body}
      {menu.element}
    </article>
  );
});
