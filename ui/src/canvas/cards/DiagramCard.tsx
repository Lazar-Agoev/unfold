/**
 * A compact card in the Diagram lens (design §4.1): one node. Its header shows the chevron that expands
 * it (a function to its Flow, an entrance to its evidence and targets), the project monogram (a card
 * from another project), the kind's mark (`ƒ` for a function, or the entrance's kind: `HTTP`, `WORKER`,
 * `SCHEDULE`, `COMMAND`), the name, and where it is; the review banner and the whole description (with
 * the agent mark when an agent wrote it) follow, then how many proposed steps it holds ("1 proposal";
 * they show in its Flow). Its arrows leave from its edge, one per card it calls. Folded, it shows only
 * its header (and no chevron).
 */

import { useQuery } from "@tanstack/react-query";
import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo } from "react";
import { Monogram } from "../../components";
import type { Card, CardView, Key, ProjectId } from "../../ipc/generated";
import { plural } from "../../lib/format";
import { cardKey, findPage, keyDisplayName, proposalsOf, useOpenBoard, type PageTarget } from "../../state/boards";
import { cardQuery, isMissing } from "../cardData";
import { entranceTitle } from "../entranceText";
import { useMeasure } from "../measure";
import { nodeLocation, nodeWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { CardGlyph, CardHandles, CardNote, DescriptionStrip, ExpandToggle, HeaderPorts, toggleCardFolded } from "./parts";
import { ReviewBanner } from "./ReviewBanner";
import { clickCard } from "./select";

export interface DiagramCardProps {
  readonly target: PageTarget;
  /** The project the card resolves to. */
  readonly project: ProjectId;
  readonly card: Card;
  /** Its ports: the node's, or every port of a folded card. */
  readonly ports: readonly string[];
  readonly selected: boolean;
}

/** What a node's header says: its mark, name, and where it is; and where its source is, for Open source. */
function nodeText(model: CardView | undefined, key: Key) {
  if (model?.type === "block") {
    const { header } = model;
    return {
      mark: "ƒ",
      name: header.qualifiedName,
      location: nodeLocation(header.path, header.startLine),
      source: { path: header.path, line: header.startLine, definition: { path: header.path, kind: header.kind, name: header.name, qualifiedName: header.qualifiedName, line: header.startLine } },
    };
  }
  if (model?.type === "entrance") {
    const { evidence } = model;
    return {
      mark: model.header.kind.toUpperCase(),
      name: entranceTitle(model),
      location: nodeLocation(evidence.path, evidence.startLine),
      source: { path: evidence.path, line: evidence.startLine },
    };
  }
  return { mark: key.startsWith("entrance:") ? "" : "ƒ", name: keyDisplayName(key), location: "", source: null };
}

export function DiagramCard({ target, project, card, ports, selected }: DiagramCardProps) {
  const key = cardKey(card);
  const entrance = "entrance" in card.ref;
  const query = useQuery(cardQuery(project, key));
  const model = query.data;
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const folded = card.folded === true;
  const text = useMemo(() => nodeText(model, key), [model, key]);
  const menu = useCardMenu(target, project, card.id, folded, !entrance, text.source);
  const proposals = useOpenBoard(target.board, (board) => {
    const page = findPage(board.document, target.page);
    return page ? proposalsOf(page, card.id).length : 0;
  }) ?? 0;
  const foreign = card.ref.project;
  const width = useMemo(
    () => nodeWidth({ mark: text.mark, name: text.name, location: text.location, monogram: foreign !== undefined, code: !entrance }, measure),
    [text, foreign, entrance, measure],
  );

  const portsKey = ports.join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, folded, width, model]);

  const missing = query.isError && isMissing(query.error);
  return (
    <article
      className={`board-card diagram-card${selected ? " selected" : ""}${folded ? " folded" : ""}${missing ? " missing" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={query.isPending ? "true" : undefined}
      style={{ width }}
      aria-label={text.name}
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
        {folded ? null : <ExpandToggle target={target} cardId={card.id} expanded={false} name={text.name} />}
        {foreign !== undefined ? <Monogram name={foreign} size={20} /> : null}
        {entrance ? <span className="ecard-method node-mark">{text.mark}</span> : <CardGlyph kind={model?.type === "block" ? model.header.kind : null} />}
        <h3 className="card-title" title={text.name}>
          {text.name}
        </h3>
        <span className="card-location" title={text.location}>
          {text.location}
        </span>
        <HeaderPorts ids={ports} />
      </header>
      <ReviewBanner target={target} project={project} cardId={card.id} />
      {folded ? null : (
        <>
          <DescriptionStrip target={target} cardId={card.id} description={card.description} wrap />
          {query.isError ? (
            <CardNote warn>{missing ? `This ${entrance ? "entrance" : "reference"} is missing from the graph.` : query.error.message}</CardNote>
          ) : null}
          {proposals > 0 ? (
            <p className="card-proposals">
              <span className="proposal-glyph" aria-hidden="true" />
              {plural(proposals, "proposal")}
            </p>
          ) : null}
        </>
      )}
      {menu.element}
    </article>
  );
}
