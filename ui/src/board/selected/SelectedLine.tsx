/**
 * Selected for a line between cards: the call as written (`chargePayment(cart, req.user)`), or the
 * passed reference, and the two cards it joins. Its contract is open on the canvas.
 */

import { useQuery } from "@tanstack/react-query";
import { cardQuery } from "../../canvas";
import { Monogram } from "../../components";
import type { Card, CardView } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { cardKey, findCard, findPage, focusCard, keyDisplayName, useOpenBoard, type PageTarget, type SelectedLine as Line } from "../../state/boards";
import { useProjectRow } from "../../state/workspace";
import { RelationRow } from "./Relations";

/** The written callee (a Code link's text, a Flow chip's name) of the line's edge in the calling card's model. */
export function writtenCallee(model: CardView | undefined, source: string, seq: number): string | null {
  if (model?.type !== "block") {
    return null;
  }
  const code = model.code?.source.state === "current" ? model.code.source : null;
  const link = seq === 0 ? code?.links.find((item) => item.source === source) : undefined;
  if (link) {
    return link.text;
  }
  for (const node of model.flow?.nodes ?? []) {
    const chip = node.chips?.find((item) => item.source === source && item.seq === seq);
    if (chip) {
      return chip.name;
    }
  }
  return null;
}

/** One end of a line: its card, which a click brings into view. */
export function End({ target, label, card, boardProject }: { readonly target: PageTarget; readonly label: string; readonly card: Card | undefined; readonly boardProject: string }) {
  if (!card) {
    return <RelationRow label={label}>a removed card</RelationRow>;
  }
  return (
    <RelationRow
      label={label}
      onActivate={() => {
        focusCard(target, card.id);
      }}
    >
      <span className="relation-card">
        <Monogram name={card.ref.project ?? boardProject} size={14} />
        <span className="relation-code">{keyDisplayName(cardKey(card))}</span>
      </span>
    </RelationRow>
  );
}

export function SelectedLine({ target, line }: { readonly target: PageTarget; readonly line: Line }) {
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const boardProject = useProjectRow(target.board.project)?.name ?? "";
  const from = page ? findCard(page, line.from) : undefined;
  const to = page ? findCard(page, line.to) : undefined;
  const model = useQuery({ ...cardQuery(line.project, from ? cardKey(from) : ""), enabled: from !== undefined });
  const contract = useEngineQuery("call_contract", { project: line.project, source: line.source, seq: line.seq });
  const callee = writtenCallee(model.data, line.source, line.seq) ?? (contract.data ? keyDisplayName(contract.data.target) : "…");
  const args = contract.data?.sends.flatMap((send) => (send.argument ? [send.argument.text ?? "…"] : []));
  const written = line.seq === 0 ? `${callee}(${args ? args.join(", ") : "…"})` : callee;
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{written}</h3>
        <span className="selected-location">
          {line.seq === 0 ? "Call" : "Passed function"}
          {line.line !== null ? ` · line ${line.line}` : ""}
        </span>
      </div>
      <div className="selected-relations link">
        <End target={target} label="From" card={from} boardProject={boardProject} />
        <End target={target} label="To" card={to} boardProject={boardProject} />
      </div>
      <p className="selected-note">What it sends, returns, and throws is open on the line. Escape closes it.</p>
    </>
  );
}
