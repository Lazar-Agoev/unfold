/**
 * An entrance card (02 §4.5): the method and trigger, where it is declared, and its handlers in
 * registration order. A resolved handler follows like a call; the opened ones (with a card on the
 * page) are accent links, and their arrows leave from their rows.
 */

import { useQuery } from "@tanstack/react-query";
import { Handle, Position, useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo } from "react";
import { Monogram } from "../../components";
import { reasonText, shortPath } from "../../code/text";
import type { Card, ProjectId, TargetView } from "../../ipc/generated";
import { editPage, entranceCardText, entranceTitle, followCall, keyDisplayName, lineOffsetIn, setCardsFolded, type PageTarget } from "../../state/boards";
import { targetHandle } from "../arrows";
import { cardQuery, isMissing } from "../cardData";
import { useMeasure } from "../measure";
import { entranceCardWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { CardHandles, CardNote, DescriptionStrip, HeaderPorts } from "./parts";
import { ReviewBanner } from "./ReviewBanner";
import { clickCard } from "./select";


export interface EntranceCardProps {
  readonly target: PageTarget;
  /** The project the card resolves to. */
  readonly project: ProjectId;
  readonly card: Card & { readonly ref: { readonly entrance: string } };
  readonly ports: readonly string[];
  readonly selected: boolean;
}

function handlerName(handler: TargetView): string {
  return handler.name ?? handler.text;
}

/** Where a handler is, or why it can't be followed (its tooltip). */
function handlerNote(handler: TargetView): string | null {
  if (handler.resolution === "external") {
    return handler.module ?? handler.note ?? "external";
  }
  if (handler.resolution === "unresolved") {
    return handler.note ?? reasonText(handler.reason);
  }
  return handler.path !== null ? `${shortPath(handler.path)}${handler.line !== null ? `:${handler.line}` : ""}` : null;
}

function HandlerRow({
  handler,
  port,
  onFollow,
}: {
  readonly handler: TargetView;
  readonly port: string | null;
  readonly onFollow: (target: string, anchor: HTMLElement) => void;
}) {
  const followable = handler.resolution === "resolved" && handler.target !== null;
  return (
    <li className="ecard-handler" data-port-row="target" data-target-key={handler.target ?? undefined}>
      {followable ? (
        <button
          type="button"
          className={`ecard-handler-name nodrag${port !== null ? " opened" : ""}`}
          title={handlerNote(handler) ?? undefined}
          onClick={(event) => {
            event.stopPropagation();
            if (handler.target !== null) {
              onFollow(handler.target, event.currentTarget);
            }
          }}
        >
          {handlerName(handler)}
        </button>
      ) : (
        <span className={`ecard-handler-name ${handler.resolution}`} title={handlerNote(handler) ?? undefined}>
          {handlerName(handler)}
        </span>
      )}
      {port !== null ? <Handle id={port} type="source" position={Position.Right} isConnectable={false} className="card-port" /> : null}
    </li>
  );
}

export function EntranceCard({ target, project, card, ports, selected }: EntranceCardProps) {
  const key = card.ref.entrance;
  const query = useQuery(cardQuery(project, key));
  const model = query.data?.type === "entrance" ? query.data : undefined;
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const folded = card.folded === true;
  const source = useMemo(() => (model ? { path: model.evidence.path, line: model.evidence.startLine } : null), [model]);
  const menu = useCardMenu(target, project, card.id, folded, false, source);
  const title = model ? entranceTitle(model) : keyDisplayName(key);
  const text = useMemo(() => (model ? entranceCardText(model) : { method: null, title, location: "", handlers: [] }), [model, title]);
  const portSet = useMemo(() => new Set(ports), [ports]);
  const foreign = card.ref.project;

  const width = useMemo(() => entranceCardWidth(text, measure), [text, measure]);

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, folded, width, model]);

  const toggleFolded = () => {
    editPage(target, (page) => {
      setCardsFolded(page, [card.id], !folded);
    });
  };
  const follow = (handler: string, anchor: HTMLElement) => {
    const offset = lineOffsetIn(anchor);
    followCall({
      target,
      fromCardId: card.id,
      fromLabel: title,
      targetKey: handler,
      targetLabel: keyDisplayName(handler),
      ...(offset !== undefined ? { lineOffset: offset } : {}),
    });
  };
  const missing = query.isError && isMissing(query.error);
  return (
    <article
      className={`board-card entrance-card${selected ? " selected" : ""}${folded ? " folded" : ""}${missing ? " missing" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={query.isPending ? "true" : undefined}
      style={{ width }}
      aria-label={title}
      onClick={() => {
        clickCard(target, card.id, null);
      }}
      onContextMenu={menu.onContextMenu}
    >
      <CardHandles />
      <header className="card-header card-drag" onDoubleClick={toggleFolded}>
        {foreign !== undefined ? <Monogram name={foreign} size={20} /> : null}
        {text.method !== null ? <span className="ecard-method">{text.method}</span> : null}
        <h3 className="card-title" title={title}>
          {text.title}
        </h3>
        <HeaderPorts ids={folded ? ports : []} />
      </header>
      <ReviewBanner target={target} project={project} cardId={card.id} />
      {folded ? null : (
        <>
          <DescriptionStrip target={target} cardId={card.id} description={card.description} />
          <div className="card-body ecard-body nodrag nopan">
              {query.isPending ? <CardNote>Loading…</CardNote> : null}
              {query.isError ? <CardNote warn>{missing ? "This entrance is missing from the graph." : query.error.message}</CardNote> : null}
              {model ? (
                <>
                  <span className="ecard-location">{text.location}</span>
                  <h4 className="ecard-heading">Handlers, in order</h4>
                  {model.targets.length === 0 ? <p className="ecard-empty">No handlers resolved.</p> : null}
                  <ul className="ecard-handlers">
                    {model.targets.map((handler) => {
                      const handle = targetHandle(handler.position);
                      return <HandlerRow key={handle} handler={handler} port={portSet.has(handle) ? handle : null} onFollow={follow} />;
                    })}
                  </ul>
                </>
              ) : null}
          </div>
        </>
      )}
      {menu.element}
    </article>
  );
}
