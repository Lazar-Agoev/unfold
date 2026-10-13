/**
 * Selected for an arrow from a Diagram node that stands for several calls to one card: its two cards,
 * then each call as written in the caller, with its line, in source order. Choosing one shows its
 * contract here.
 */

import { useQuery } from "@tanstack/react-query";
import { cardQuery } from "../../canvas";
import { plural } from "../../lib/format";
import { cardKey, findCard, findPage, keyDisplayName, select, useOpenBoard, type PageTarget, type SelectedCalls as Calls } from "../../state/boards";
import { RelationRow } from "./Relations";
import { End, writtenCallee } from "./SelectedLine";

export function SelectedCalls({ target, calls }: { readonly target: PageTarget; readonly calls: Calls }) {
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const from = page ? findCard(page, calls.from) : undefined;
  const to = page ? findCard(page, calls.to) : undefined;
  const model = useQuery({ ...cardQuery(target.board.project, from ? cardKey(from) : ""), enabled: from !== undefined });
  const callee = to ? keyDisplayName(cardKey(to)) : "the callee";
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{plural(calls.calls.length, "call")}</h3>
        <span className="selected-location">To {callee}</span>
      </div>
      <div className="selected-relations link">
        <End target={target} label="From" card={from} />
        <End target={target} label="To" card={to} />
      </div>
      <div className="selected-relations">
        {calls.calls.map((call, index) => {
          const written = writtenCallee(model.data, call.source, call.seq) ?? callee;
          return (
            <RelationRow
              key={`${call.source}:${call.seq}`}
              {...(index === 0 ? { label: "Calls" } : {})}
              onActivate={() => {
                select(target, { kind: "line", arrowId: calls.arrowId, source: call.source, seq: call.seq, from: calls.from, to: calls.to, line: call.line });
              }}
            >
              <span className="relation-code">{call.seq === 0 ? `${written}(…)` : written}</span>
              {call.line !== null ? <span className="relation-path">{` · line ${call.line}`}</span> : null}
            </RelationRow>
          );
        })}
      </div>
    </>
  );
}
