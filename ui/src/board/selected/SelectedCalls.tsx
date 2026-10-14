/**
 * Selected for an arrow that stands for several calls to one card: its two cards, then each call as
 * written, with its line, in the order they run. Choosing one shows its contract here.
 */

import { useQueries } from "@tanstack/react-query";
import { engineQueryOptions } from "../../ipc/queries";
import { plural } from "../../lib/format";
import { focusCard, select, type PageTarget, type SelectedCalls as Calls } from "../../state/boards";
import { useCardNames } from "../../state/cards";
import { RelationRow } from "./Relations";

export function SelectedCalls({ target, calls }: { readonly target: PageTarget; readonly calls: Calls }) {
  const names = useCardNames(target);
  const contracts = useQueries({ queries: calls.calls.map((key) => engineQueryOptions("call_contract", { project: target.board.project, key })) });
  const end = (label: string, cardId: string) => (
    <RelationRow
      label={label}
      onActivate={() => {
        focusCard(target, cardId);
      }}
    >
      <span className="relation-code">{names(cardId)}</span>
    </RelationRow>
  );
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{plural(calls.calls.length, "call")}</h3>
        <span className="selected-location">To {names(calls.to)}</span>
      </div>
      <div className="selected-relations link">
        {end("From", calls.from)}
        {end("To", calls.to)}
      </div>
      <div className="selected-relations">
        {calls.calls.map((call, index) => {
          const contract = contracts[index]?.data;
          return (
            <RelationRow
              key={call}
              {...(index === 0 ? { label: "Calls" } : {})}
              onActivate={() => {
                select(target, { kind: "line", arrowId: calls.arrowId, call, from: calls.from, to: calls.to });
              }}
            >
              <span className="relation-code">{contract?.text ?? "…"}</span>
              {contract ? <span className="relation-path">{` · line ${contract.line}`}</span> : null}
            </RelationRow>
          );
        })}
      </div>
    </>
  );
}
