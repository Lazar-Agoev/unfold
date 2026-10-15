/**
 * An app's or entrance's card: it looks declared, so it is drawn with a dark border. An entrance's card shows
 * its app in small caps, its name, and its `on` in mono; an app's card, its name and its `run`. It points
 * at the cards for its code, in order (`computeArrows`).
 *
 * Compact, its arrows leave from its edge. Expanded, it says its one sentence and lists its code in
 * order, each row with where it is, and each arrow leaves from its row. Code that isn't found is drawn
 * beside it as its own dashed card, with the fix the engine offers (`UnfoundCode`); stale code is its
 * last good block's card, marked stale, its arrow dashed.
 *
 * Its `</>` opens its lines in `unfold.toml` in the code panel, and a line of it hovered there lights
 * the card.
 */

import type { CardShape } from "@unfold/desktop/drawing";
import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo } from "react";
import { CodeButton } from "../../code/CodeButton";
import { viewCode } from "../../code/open";
import type { Card, StartCard } from "../../ipc/generated";
import { clickItem, type PageTarget } from "../../state/boards";
import { useCodeHover } from "../../state/codePanel";
import { codePort, NODE_PORT } from "../arrows";
import { startRowName, startRowPlace, startRows, unfoundCode } from "../cardText";
import { useMeasure } from "../measure";
import { startCardWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { CardHandles, DescriptionStrip, ExpandToggle, HeaderPorts, renderPorts } from "./parts";
import { ReviewBanner } from "./ReviewBanner";
import { UnfoundCode } from "./UnfoundCode";

export interface EntranceCardProps {
  readonly target: PageTarget;
  readonly card: Card;
  readonly start: StartCard & { readonly type: "start" };
  readonly shape: CardShape;
  readonly ports: readonly string[];
  readonly selected: boolean;
}

export function EntranceCard({ target, card, start, shape, ports, selected }: EntranceCardProps) {
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const menu = useCardMenu(target, card, false);
  const expanded = shape === "expanded";
  const width = useMemo(() => startCardWidth(start, shape, measure), [start, shape, measure]);
  const rows = startRows(start, shape);
  const unfound = unfoundCode(start);
  const entrance = start.kind === "entrance";
  const portSet = useMemo(() => new Set(ports), [ports]);
  const lit = useCodeHover()?.key === start.key;

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, width, start, shape]);

  const toggle = <ExpandToggle target={target} cardId={card.id} expanded={expanded} name={start.name} />;
  const name = <strong className="start-name">{start.name}</strong>;
  return (
    <div className="start-node">
      <article
        className={`board-card start-card${selected ? " selected" : ""}${lit ? " lit" : ""}`}
        data-card-id={card.id}
        data-key={start.key}
        style={{ width }}
        aria-label={entrance ? `${start.app} · ${start.name}` : start.name}
        onClick={() => {
          clickItem(target, card.id, { kind: "card", cardId: card.id, key: null });
        }}
        onContextMenu={menu.onContextMenu}
      >
        <CardHandles />
        <header className="start-head card-drag">
          {entrance ? (
            <>
              <span className="start-app">
                {toggle}
                <small>{start.app}</small>
              </span>
              {name}
            </>
          ) : (
            <span className="start-app">
              {toggle}
              {name}
            </span>
          )}
          {start.on !== undefined ? <code className="start-on">{start.on}</code> : null}
          {expanded && start.about !== undefined ? <p className="start-about">{start.about}</p> : null}
          <CodeButton
            className="card-view-code"
            onView={() => {
              viewCode(target, { kind: "card", cardId: card.id, key: null }, new Map([[card.id, start]]));
            }}
          />
          <HeaderPorts ids={ports.filter((id) => id === NODE_PORT)} />
        </header>
        <ReviewBanner target={target} cardId={card.id} view={start} />
        <DescriptionStrip target={target} cardId={card.id} description={card.description} wrap />
        {rows.length > 0 ? (
          <ul className="start-codes">
            {rows.map(({ code, index }) => {
              const place = startRowPlace(code);
              const port = codePort(index);
              return (
                <li key={index} className="start-code">
                  <span className="start-code-name">{startRowName(code)}</span>
                  {place !== null ? <span className="start-code-place">{place}</span> : null}
                  {code.state === "stale" ? <span className="start-code-state">stale</span> : null}
                  {portSet.has(port) ? renderPorts([port]) : null}
                </li>
              );
            })}
          </ul>
        ) : null}
        {menu.element}
      </article>
      {unfound.length > 0 ? (
        <div className="start-unfound">
          {unfound.map(({ code, index }) => (
            <UnfoundCode key={index} project={target.board.project} start={start.key} code={code} index={index} />
          ))}
        </div>
      ) : null}
    </div>
  );
}
