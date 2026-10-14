/**
 * Selected for a call's arrow: the call as written (`charge_card(order["card"], total)`) and its line,
 * then its contract, by the call's block key: what it sends (each argument beside the parameter it
 * fills), what comes back and where it lands, and what the callee raises ("caught" when a handler
 * around the call catches it).
 */

import type { CallContract, ContractRaise, ContractReturn, ContractSend } from "../../ipc/generated";
import { useEngineQuery } from "../../ipc/queries";
import { type PageTarget, type SelectedLine as Line } from "../../state/boards";
import { RelationRow } from "./Relations";

/** A side of a contract row: what it says, and whether it reads as a gap (no value written). */
interface ContractText {
  readonly text: string;
  readonly dim: boolean;
}

/** Where the result lands. */
function landingText({ lands }: ContractReturn): ContractText {
  switch (lands.kind) {
    case "binds":
      return { text: lands.target, dim: false };
    case "exit":
      return { text: lands.form, dim: false };
    case "argument":
      return { text: "an argument", dim: false };
    case "receiver":
      return { text: "a receiver", dim: false };
    case "nowhere":
      return { text: "—", dim: true };
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

/** What a raise throws: its type, else its value as written. */
function raisedText(raise: ContractRaise): string {
  return raise.type?.text ?? raise.value ?? "?";
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
        <Side value={landingText(returns)} />
      </RelationRow>
      {contract.raises.map((raise, index) => (
        <RelationRow key={raise.exit} label={index === 0 ? "Throws" : ""} title={raisedText(raise)}>
          {raisedText(raise)}
          {raise.caughtBy !== undefined ? <span className="relation-note">caught</span> : null}
        </RelationRow>
      ))}
    </div>
  );
}

export function SelectedLine({ target, line }: { readonly target: PageTarget; readonly line: Line }) {
  const contract = useEngineQuery("call_contract", { project: target.board.project, key: line.call });
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{contract.data?.text ?? "…"}</h3>
        <span className="selected-location ui">{contract.data ? `Call · line ${contract.data.line}` : "Call"}</span>
      </div>
      {contract.data ? <Contract contract={contract.data} /> : <p className="selected-note">{contract.isError ? contract.error.message : "Reading the call…"}</p>}
    </>
  );
}
