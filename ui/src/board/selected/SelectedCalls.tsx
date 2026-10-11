/**
 * Selected for an arrow from a Diagram node that stands for several calls to one card: its two cards,
 * then each call as written in the caller, with its line, in source order. Each opens its contract on
 * the arrow.
 */

import { useQuery } from "@tanstack/react-query";
import { cardQuery } from "../../canvas";
import { countText } from "../../canvas/lines/labelText";
import { cardKey, findCard, findPage, keyDisplayName, select, useOpenBoard, type PageTarget, type SelectedCalls as Calls } from "../../state/boards";
import { useProjectRow } from "../../state/workspace";
import { RelationRow } from "./Relations";
import { End, writtenCallee } from "./SelectedLine";

export function SelectedCalls({ target, calls }: { readonly target: PageTarget; readonly calls: Calls }) {
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const boardProject = useProjectRow(target.board.project)?.name ?? "";
  const from = page ? findCard(page, calls.from) : undefined;
  const to = page ? findCard(page, calls.to) : undefined;
  const model = useQuery({ ...cardQuery(calls.project, from ? cardKey(from) : ""), enabled: from !== undefined });
  const callee = to ? keyDisplayName(cardKey(to)) : "the callee";
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{countText(calls.calls.length)}</h3>
        <span className="selected-location">To {callee}</span>
      </div>
      <div className="selected-relations link">
        <End target={target} label="From" card={from} boardProject={boardProject} />
        <End target={target} label="To" card={to} boardProject={boardProject} />
      </div>
      <div className="selected-relations">
        {calls.calls.map((call, index) => {
          const written = writtenCallee(model.data, call.source, call.seq) ?? callee;
          return (
            <RelationRow
              key={`${call.source}:${call.seq}`}
              {...(index === 0 ? { label: "Calls" } : {})}
              onActivate={() => {
                select(target, { kind: "line", arrowId: calls.arrowId, project: calls.project, source: call.source, seq: call.seq, from: calls.from, to: calls.to, line: call.line });
              }}
            >
              <span className="relation-code">{call.seq === 0 ? `${written}(…)` : written}</span>
              {call.line !== null ? <span className="relation-path">{` · line ${call.line}`}</span> : null}
            </RelationRow>
          );
        })}
      </div>
      <p className="selected-note">Choose a call to open what it sends, returns, and throws on the line.</p>
    </>
  );
}
