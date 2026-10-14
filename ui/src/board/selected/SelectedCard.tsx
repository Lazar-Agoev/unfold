/**
 * Selected for a card: its name and where it is, its description (edited here or on the card), then for
 * a module or definition Called by and Reached from (up to 3 each, then "N more · Show callers"). A
 * class or type says where it is used ("Used in N places", a popover of the places). An app's or
 * entrance's card lists the code it starts at.
 */

import { showCallers } from "../../canvas";
import { CARD_DESCRIPTION_FIELD } from "../../canvas/cards/blockMenu";
import { UsedIn } from "../../code/UsedIn";
import { shortPath } from "../../code/text";
import { TextButton } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { Card, CardView, ProjectId, StartCard, TargetState } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import { addBlock, addRef, cardKey, describeCard, itemsFor, useOpenBoard, type PageTarget } from "../../state/boards";
import { cardName, useCard } from "../../state/cards";
import { AuthoredField } from "./AuthoredField";
import { RelationRow } from "./Relations";

/** How many callers and entrances Selected lists before "N more". */
const RELATIONS_SHOWN = 3;

/** What an app's or entrance's code key names, when it isn't found. */
const STATE_WORDS: Readonly<Record<Exclude<TargetState, "found">, string>> = {
  missing: "missing",
  not_analyzed: "not analyzed",
  not_a_start: "not a start",
  stale: "stale",
};

/** Called by and Reached from, each bringing its card onto the page (or into view). */
function Relations({ target, cardId, blockKey }: { readonly target: PageTarget; readonly cardId: string; readonly blockKey: string }) {
  const { project } = target.board;
  const relations = useEngineQuery("relations", { project, key: blockKey, callersLimit: RELATIONS_SHOWN });
  if (relations.isPending) {
    return <p className="selected-note">Loading…</p>;
  }
  if (relations.isError) {
    return <p className="selected-note">{relations.error.message}</p>;
  }
  const { callers, callersTotal, reachedFrom } = relations.data;
  return (
    <div className="selected-relations">
      {callers.map((caller, index) => (
        <RelationRow
          key={caller.call}
          {...(index === 0 ? { label: "Called by" } : {})}
          title={`${shortPath(caller.path)}:${caller.line}`}
          onActivate={() => {
            addBlock(target, caller.owner, caller.ownerName);
          }}
        >
          {caller.ownerName}
        </RelationRow>
      ))}
      {callersTotal > callers.length ? (
        <RelationRow>
          <span className="relation-more">
            {callersTotal - callers.length} more ·{" "}
            <TextButton
              onClick={() => {
                void showCallers(target, cardId);
              }}
            >
              Show callers
            </TextButton>
          </span>
        </RelationRow>
      ) : null}
      {reachedFrom.slice(0, RELATIONS_SHOWN).map((entrance, index) => (
        <RelationRow
          key={entrance.key}
          {...(index === 0 ? { label: "Reached from" } : {})}
          wrap
          onActivate={() => {
            addRef(target, { entrance: entrance.key }, entrance.name);
          }}
        >
          {`${entrance.app} › ${entrance.name}`}
        </RelationRow>
      ))}
      {reachedFrom.length > RELATIONS_SHOWN ? (
        <RelationRow>
          <span className="relation-more">{reachedFrom.length - RELATIONS_SHOWN} more</span>
        </RelationRow>
      ) : null}
    </div>
  );
}

/** Where a class or type is written: "Used in N places", opening the places. */
function TypeUses({ project, typeKey }: { readonly project: ProjectId; readonly typeKey: string }) {
  const definition = useEngineQuery("type_definition", { project, key: typeKey });
  if (!definition.data) {
    return null;
  }
  return (
    <div className="selected-relations">
      <RelationRow label="Uses">
        <UsedIn project={project} definition={definition.data} />
      </RelationRow>
    </div>
  );
}

/** The code an app or entrance starts at, in the order it runs: each found block brings its card onto the page. */
function StartsAt({ target, start }: { readonly target: PageTarget; readonly start: StartCard }) {
  return (
    <div className="selected-relations">
      {start.code.map((code, index) => {
        const label = code.label ?? code.key;
        return code.state === "found" ? (
          <RelationRow
            key={code.key}
            {...(index === 0 ? { label: "Starts at" } : {})}
            {...(code.path !== undefined ? { title: `${shortPath(code.path)}${code.line !== undefined ? `:${code.line}` : ""}` } : {})}
            onActivate={() => {
              addBlock(target, code.key, label);
            }}
          >
            {label}
          </RelationRow>
        ) : (
          <RelationRow key={code.key} {...(index === 0 ? { label: "Starts at" } : {})} title={code.key}>
            <span className="relation-dim">{`${label} · ${STATE_WORDS[code.state]}`}</span>
          </RelationRow>
        );
      })}
    </div>
  );
}

/** Where a card's code is: a definition's `file:line`, or an entrance's app and what starts it. */
function location(view: CardView): string {
  if (view.type === "definition") {
    return `${view.flow.header.path}:${view.flow.header.line}`;
  }
  return view.kind === "entrance" ? [view.app, view.on].filter((part) => part !== undefined).join(" · ") : (view.on ?? "");
}

export function SelectedCard({ target, card }: { readonly target: PageTarget; readonly card: Card }) {
  const key = cardKey(card);
  const { project } = target.board;
  const author = useAuthor();
  const query = useCard(target, card);
  const health = useOpenBoard(target.board, (board) => board.health);
  const issues = itemsFor(health, target.page, { card: card.id });
  const held = issues.some((item) => item.kind === "review");
  const view = query.data;
  const kind = view?.type === "definition" ? view.flow.header.kind : undefined;
  const disabledReason = held ? "Held for review: accept or reconnect it on the card first." : author === null ? "Loading who you are…" : null;
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{view ? cardName(view) : query.isError ? query.error.message : "Loading…"}</h3>
        {view ? <p className="selected-location">{location(view)}</p> : null}
      </div>
      <AuthoredField
        label="Description"
        value={card.description}
        placeholder="What this does, in a sentence…"
        focusKey={CARD_DESCRIPTION_FIELD}
        disabledReason={disabledReason}
        status={issues.map((item) => (
          <span key={item.message} className="authored-note">
            {item.message}
          </span>
        ))}
        onChange={(text) => {
          if (author !== null) {
            describeCard(target, card.id, text, author);
          }
        }}
      />
      {kind === "class" || kind === "type" ? <TypeUses project={project} typeKey={key} /> : null}
      {kind !== undefined && kind !== "type" ? <Relations target={target} cardId={card.id} blockKey={key} /> : null}
      {view?.type === "start" ? <StartsAt target={target} start={view} /> : null}
    </>
  );
}
