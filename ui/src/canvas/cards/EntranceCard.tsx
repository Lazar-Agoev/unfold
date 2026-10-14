/**
 * An app's or entrance's card (entrance-definitions.md, "On a board"; core-scope.html's entrance): it
 * looks declared, so it is drawn in the authored style. It shows the app in small caps, the name, and
 * what starts it in mono: an entrance's `on`, an app's `run`. It points at the cards for its code, in
 * order (`computeArrows`).
 *
 * Compact, its arrows leave from its edge. Expanded, it says its one sentence and lists its code in
 * order, each row with where it is, and each arrow leaves from its row. Code that isn't found is
 * always listed, dashed and labeled with its state (missing, not analyzed, not a start, stale).
 */

import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo } from "react";
import type { Card, StartCard } from "../../ipc/generated";
import { select, type PageTarget } from "../../state/boards";
import { codePort, NODE_PORT } from "../arrows";
import { TARGET_STATE_WORDS, startRowName, startRowPlace, startRows, type CardShape } from "../cardText";
import { useMeasure } from "../measure";
import { startCardWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { CardHandles, DescriptionStrip, ExpandToggle, HeaderPorts, renderPorts } from "./parts";
import { ReviewBanner } from "./ReviewBanner";

export interface EntranceCardProps {
  readonly target: PageTarget;
  readonly card: Card;
  readonly start: StartCard & { readonly type: "start" };
  readonly shape: CardShape;
  readonly ports: readonly string[];
  readonly selected: boolean;
}

/** What starts an entrance, its first word (`POST`) in bold when it has more than one. */
function OnText({ on, entrance }: { readonly on: string; readonly entrance: boolean }) {
  const space = on.indexOf(" ");
  if (!entrance || space < 0) {
    return <code className="start-on">{on}</code>;
  }
  return (
    <code className="start-on">
      <b>{on.slice(0, space)}</b>
      {on.slice(space)}
    </code>
  );
}

export function EntranceCard({ target, card, start, shape, ports, selected }: EntranceCardProps) {
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const menu = useCardMenu(target, card.id, false);
  const expanded = shape === "expanded";
  const width = useMemo(() => startCardWidth(start, shape, measure), [start, shape, measure]);
  const rows = startRows(start, shape);
  const portSet = useMemo(() => new Set(ports), [ports]);

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, width, start, shape]);

  return (
    <article
      className={`board-card start-card${selected ? " selected" : ""}`}
      data-card-id={card.id}
      data-key={start.key}
      style={{ width }}
      aria-label={`${start.app} · ${start.name}`}
      onClick={() => {
        select(target, { kind: "card", cardId: card.id, key: null });
      }}
      onContextMenu={menu.onContextMenu}
    >
      <CardHandles />
      <header className="start-head card-drag">
        <span className="start-app">
          <ExpandToggle target={target} cardId={card.id} expanded={expanded} name={start.name} />
          <small>{start.app}</small>
        </span>
        <strong className="start-name">{start.name}</strong>
        {start.on !== undefined ? <OnText on={start.on} entrance={start.kind === "entrance"} /> : null}
        {expanded && start.about !== undefined ? <p className="start-about">{start.about}</p> : null}
        <HeaderPorts ids={ports.filter((id) => id === NODE_PORT)} />
      </header>
      <ReviewBanner target={target} cardId={card.id} view={start} />
      <DescriptionStrip target={target} cardId={card.id} description={card.description} wrap={!expanded} />
      {rows.length > 0 ? (
        <ul className="start-codes">
          {rows.map(({ code, index }) => {
            const place = startRowPlace(code);
            const port = codePort(index);
            return (
              <li key={index} className={`start-code${code.state === "found" ? "" : " unfound"}`}>
                <span className="start-code-name">{startRowName(code)}</span>
                {place !== null ? <span className="start-code-place">{place}</span> : null}
                {code.state !== "found" ? <span className="start-code-state">{TARGET_STATE_WORDS[code.state]}</span> : null}
                {portSet.has(port) ? renderPorts([port]) : null}
              </li>
            );
          })}
        </ul>
      ) : null}
      {menu.element}
    </article>
  );
}
