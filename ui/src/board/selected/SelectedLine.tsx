/**
 * Selected for an arrow's call: the call as written (`chargePayment(cart, req.user)`) and its line,
 * then its contract: what it sends (each argument beside the parameter it fills), what comes back and
 * where it lands, and what the callee throws ("caught" when the call sits in a `try` that catches). A
 * passed reference has no contract: Selected names the two cards it joins.
 */

import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { cardQuery } from "../../canvas";
import type { Card, CallContract, CardView, ContractReturn, ContractSend } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { cardKey, findCard, findPage, focusCard, keyDisplayName, useOpenBoard, type PageTarget, type SelectedLine as Line } from "../../state/boards";
import { RelationRow } from "./Relations";

/** A side of a contract row: what it says, and whether it reads as a gap (no value written). */
interface ContractText {
  readonly text: string;
  readonly dim: boolean;
}

/** Where the result lands. */
function bindingText(returns: ContractReturn): ContractText {
  switch (returns.binding) {
    case "variable":
      return returns.text !== undefined ? { text: returns.text, dim: false } : { text: "…", dim: true };
    case "return":
      return { text: "return", dim: false };
    case "argument":
      return returns.text !== undefined ? { text: `${returns.text}(…)`, dim: false } : { text: "…", dim: true };
    case "discard":
      return { text: "—", dim: true };
    case "other":
      return { text: "…", dim: true };
  }
}

/** What one send passes: the argument as written, or why none fills the parameter. */
function sentText(send: ContractSend): ContractText {
  if (send.argument) {
    return send.argument.text !== undefined ? { text: send.argument.text, dim: false } : { text: "…", dim: true };
  }
  const parameter = send.parameter;
  return { text: parameter?.default === true ? "default" : parameter?.optional === true ? "optional" : "—", dim: true };
}

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
export function End({ target, label, card }: { readonly target: PageTarget; readonly label: string; readonly card: Card | undefined }) {
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
      <span className="relation-code">{keyDisplayName(cardKey(card))}</span>
    </RelationRow>
  );
}

function Side({ value }: { readonly value: ContractText }) {
  return <span className={value.dim ? "relation-dim" : undefined}>{value.text}</span>;
}

function Arrow() {
  return <span className="relation-arrow">→</span>;
}

/** One send: the argument, and the parameter it fills (or why it fills none). */
function Send({ send, label }: { readonly send: ContractSend; readonly label: string }) {
  const parameter = send.parameter?.name;
  const sent = sentText(send);
  return (
    <RelationRow label={label} title={parameter !== undefined ? `${sent.text} → ${parameter}` : sent.text}>
      <Side value={sent} />
      {parameter !== undefined ? (
        <>
          <Arrow />
          {parameter}
        </>
      ) : null}
      {send.unpaired ? <span className="relation-note">{send.unpaired.note}</span> : null}
    </RelationRow>
  );
}

function Contract({ contract }: { readonly contract: CallContract }) {
  const { returns } = contract;
  return (
    <div className="selected-relations link call">
      {contract.sends.length === 0 ? (
        <RelationRow label="Sends">
          <span className="relation-dim">nothing</span>
        </RelationRow>
      ) : (
        contract.sends.map((send, index) => <Send key={index} send={send} label={index === 0 ? "Sends" : ""} />)
      )}
      <RelationRow label="Returns">
        <Side value={returns.type ? { text: returns.type.text, dim: false } : { text: "?", dim: true }} />
        <Arrow />
        <Side value={bindingText(returns)} />
      </RelationRow>
      {contract.throws.map((thrown, index) => (
        <RelationRow key={index} label={index === 0 ? "Throws" : ""} title={thrown.type.text}>
          {thrown.type.text}
          {contract.caught ? <span className="relation-note">caught</span> : null}
        </RelationRow>
      ))}
    </div>
  );
}

export function SelectedLine({ target, line }: { readonly target: PageTarget; readonly line: Line }) {
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const from = page ? findCard(page, line.from) : undefined;
  const to = page ? findCard(page, line.to) : undefined;
  const call = line.seq === 0;
  const model = useQuery({ ...cardQuery(target.board.project, from ? cardKey(from) : ""), enabled: from !== undefined });
  const contract = useEngineQuery("call_contract", { project: target.board.project, source: line.source, seq: line.seq }, { enabled: call });
  const callee = writtenCallee(model.data, line.source, line.seq) ?? (contract.data ? keyDisplayName(contract.data.target) : "…");
  const args = contract.data?.sends.flatMap((send) => (send.argument ? [send.argument.text ?? "…"] : []));
  const written = call ? `${callee}(${args ? args.join(", ") : "…"})` : callee;
  let body: ReactNode;
  if (!call) {
    body = (
      <div className="selected-relations link">
        <End target={target} label="From" card={from} />
        <End target={target} label="To" card={to} />
      </div>
    );
  } else if (contract.data) {
    body = <Contract contract={contract.data} />;
  } else {
    body = <p className="selected-note">{contract.isError ? contract.error.message : "Reading the call…"}</p>;
  }
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{written}</h3>
        <span className="selected-location ui">
          {call ? "Call" : "Passed function"}
          {line.line !== null ? ` · line ${line.line}` : ""}
        </span>
      </div>
      {body}
    </>
  );
}
