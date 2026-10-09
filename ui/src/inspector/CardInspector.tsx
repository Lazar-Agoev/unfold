/**
 * The inspector for a selected card (architecture §11.5, U-29): its description, and for a block
 * card its Calls (on the board, **+ Open**, or external and unresolved with their module or
 * reason), Called by, Reached from, and Groups with their descriptions. For an entrance card, its
 * handlers. Never code.
 */

import { useQuery } from "@tanstack/react-query";
import { useMemo, type ReactNode } from "react";
import { openBoardDialog } from "../boards/dialogs";
import { cardQuery } from "../canvas/cardData";
import { shortPath } from "../code/text";
import type { Card, Group, HealthItem, OutgoingCall } from "../ipc/generated";
import { useEngineQuery } from "../ipc/queries";
import { describeCard, editGroup, focusCard, openBlockCard, openEntrance, removeCardFromBoard, ungroup } from "../state/boards/api";
import { cardKey, findCardByKey, keyDisplayName } from "../state/boards/document";
import { itemsFor } from "../state/boards/health";
import { followCall } from "../state/boards/navigation";
import { getBoard, pushFocusMove, useBoards } from "../state/boards/store";
import { AuthoredField } from "./AuthoredField";
import { IssueLine } from "./IssueLine";
import { useAuthor } from "./useAuthor";

export function Section({ title, count, children }: { readonly title: string; readonly count?: number | undefined; readonly children: ReactNode }) {
  return (
    <section className="inspector-section">
      <h3 className="inspector-heading">
        {title}
        {count !== undefined ? <span className="inspector-count">{count}</span> : null}
      </h3>
      {children}
    </section>
  );
}

function isOnBoard(boardId: string, key: string): boolean {
  const board = getBoard(boardId);
  return board !== undefined && findCardByKey(board.document, key) !== undefined;
}

function focusOnBoard(boardId: string, key: string, label: string): void {
  const board = getBoard(boardId);
  const card = board ? findCardByKey(board.document, key) : undefined;
  if (card) {
    pushFocusMove(boardId, [{ cardId: card.id, label }]);
    focusCard(boardId, card.id);
  }
}

/** One entry per target (several calls to one function are one row), in source order. */
function uniqueTargets(calls: readonly OutgoingCall[]): OutgoingCall[] {
  const seen = new Set<string>();
  return calls.filter((call) => {
    const id = call.target ?? `${call.source}:${call.seq}`;
    if (seen.has(id)) {
      return false;
    }
    seen.add(id);
    return true;
  });
}

function Calls({ boardId, card, blockKey, name }: { readonly boardId: string; readonly card: Card; readonly blockKey: string; readonly name: string }) {
  const cards = useBoards((state) => state.boards[boardId]?.document.cards);
  const onBoard = useMemo(() => [...new Set((cards ?? []).map(cardKey))].sort(), [cards]);
  const relations = useEngineQuery("relations", { key: blockKey, onBoard });
  if (relations.isPending) {
    return <p className="inspector-hint">Loading…</p>;
  }
  if (relations.isError) {
    return <p className="inspector-hint">{relations.error.message}</p>;
  }
  const { outgoing, targetNames, callers, callersTotal, reachedFrom } = relations.data;
  const nameOf = (call: OutgoingCall) => (call.target !== undefined ? (targetNames[call.target] ?? keyDisplayName(call.target)) : call.text);
  const placed = uniqueTargets(outgoing.onBoard);
  const openable = uniqueTargets(outgoing.openable);
  // One row per written call and origin: `res.status` written three times is one row.
  const others = outgoing.externalOrUnresolved.filter(
    (call, index, all) => all.findIndex((other) => other.text === call.text && other.resolution === call.resolution && (other.module ?? other.note) === (call.module ?? call.note)) === index,
  );
  const callCount = placed.length + openable.length + others.length;
  return (
    <>
      <Section title="Calls" count={callCount}>
        {callCount === 0 ? <p className="inspector-hint">No calls.</p> : null}
        <ul className="inspector-list">
          {placed.map((call) => (
            <li key={`b:${call.target ?? call.source}`}>
              <button type="button" className="inspector-row" onClick={() => { if (call.target !== undefined) focusOnBoard(boardId, call.target, nameOf(call)); }}>
                <span className="inspector-dot" aria-label="On the board">●</span>
                <span className="inspector-name">{nameOf(call)}</span>
              </button>
            </li>
          ))}
          {openable.map((call) => (
            <li key={`o:${call.target ?? call.source}`} className="inspector-row static">
              <span className="inspector-name">{nameOf(call)}</span>
              <button
                type="button"
                className="inspector-action"
                aria-label={`Open ${nameOf(call)} on the board`}
                onClick={() => {
                  if (call.target !== undefined) {
                    followCall({ boardId, fromCardId: card.id, fromLabel: name, targetKey: call.target, targetLabel: nameOf(call), line: call.line });
                  }
                }}
              >
                + Open
              </button>
            </li>
          ))}
          {others.map((call) => (
            <li key={`x:${call.source}:${call.seq}`} className={`inspector-row static ${call.resolution}`}>
              <span className={`inspector-name code-name ${call.resolution}`}>{call.text}</span>
              <span className="inspector-note">{call.resolution === "external" ? (call.module ?? call.note ?? "external") : (call.note ?? "unresolved")}</span>
            </li>
          ))}
        </ul>
      </Section>
      <Section title="Called by" count={callersTotal}>
        {callers.length === 0 ? <p className="inspector-hint">Nothing in the graph calls this.</p> : null}
        <ul className="inspector-list">
          {callers.map((caller) => (
            <li key={`${caller.source}:${caller.kind}`}>
              <button
                type="button"
                className="inspector-row"
                onClick={() => {
                  if (isOnBoard(boardId, caller.owner)) {
                    focusOnBoard(boardId, caller.owner, caller.ownerName);
                  } else {
                    openBlockCard(caller.owner);
                  }
                }}
              >
                <span className="inspector-name">{caller.ownerName}</span>
                <span className="inspector-note">
                  {shortPath(caller.path)}:{caller.line}
                </span>
              </button>
            </li>
          ))}
        </ul>
        {callersTotal > callers.length ? <p className="inspector-hint">and {callersTotal - callers.length} more (see References)</p> : null}
      </Section>
      <Section title="Reached from" count={reachedFrom.length}>
        {reachedFrom.length === 0 ? <p className="inspector-hint">No entrance reaches this.</p> : null}
        <ul className="inspector-list">
          {reachedFrom.map((entrance) => (
            <li key={entrance.key}>
              <button type="button" className="inspector-row" onClick={() => { openEntrance(entrance.key); }}>
                {entrance.method !== null ? <span className="inspector-method">{entrance.method}</span> : null}
                <span className="inspector-name">{entrance.trigger ?? entrance.title}</span>
              </button>
            </li>
          ))}
        </ul>
      </Section>
    </>
  );
}

function GroupEditor({ boardId, group, author, issues }: { readonly boardId: string; readonly group: Group; readonly author: string | null; readonly issues: readonly HealthItem[] }) {
  return (
    <li className="inspector-group">
      <div className="inspector-group-head">
        <input
          className="inspector-group-title"
          aria-label={`Title of the group ${group.title}`}
          value={group.title}
          onChange={(event) => {
            if (event.target.value.trim().length > 0) {
              editGroup(boardId, group.id, { title: event.target.value });
            }
          }}
        />
        <button
          type="button"
          className="inspector-action"
          aria-pressed={group.collapsed}
          onClick={() => {
            editGroup(boardId, group.id, { collapsed: !group.collapsed });
          }}
        >
          {group.collapsed ? "Expand" : "Collapse"}
        </button>
        <button
          type="button"
          className="inspector-action danger"
          onClick={() => {
            ungroup(boardId, group.id);
          }}
        >
          Ungroup
        </button>
      </div>
      <AuthoredField
        label="Description"
        value={group.description}
        placeholder="Why these lines belong together…"
        focusKey={`inspector.group:${group.id}`}
        disabledReason={author === null ? "Loading who you are…" : null}
        status={issues.map((item) => <IssueLine key={item.message} boardId={boardId} item={item} />)}
        onChange={(text) => {
          if (author !== null) {
            editGroup(boardId, group.id, { description: { text, author } });
          }
        }}
      />
    </li>
  );
}

export function CardInspector({ boardId, card }: { readonly boardId: string; readonly card: Card }) {
  const key = cardKey(card);
  const entrance = "entrance" in card.ref;
  const author = useAuthor();
  const query = useQuery(cardQuery(key));
  const health = useBoards((state) => state.boards[boardId]?.health);
  const allGroups = useBoards((state) => state.boards[boardId]?.document.groups);
  const groups = useMemo(() => (allGroups ?? []).filter((group) => group.card === card.id), [allGroups, card.id]);
  const issues = itemsFor(health, { card: card.id });
  const held = issues.some((item) => item.kind === "review");
  const model = query.data;
  const name = model?.type === "block" ? model.header.qualifiedName : keyDisplayName(key);
  const location = model?.type === "block" ? `${shortPath(model.header.path)}:${model.header.startLine}` : model?.type === "entrance" ? shortPath(model.evidence.path) : "";
  const kind = model?.type === "block" ? model.header.kind : model?.type === "entrance" ? model.header.kind : null;

  return (
    <div className="inspector-body">
      <header className="inspector-title">
        <h3>{name}</h3>
        <p>
          {kind !== null ? <span className="inspector-kind">{kind}</span> : null}
          {location}
        </p>
      </header>
      <AuthoredField
        label="Description"
        value={card.description}
        placeholder="What this does, in a sentence…"
        focusKey="inspector.description"
        disabledReason={held ? "Held for review: accept or reconnect it before describing it." : author === null ? "Loading who you are…" : null}
        status={issues.map((item) => <IssueLine key={item.message} boardId={boardId} item={item} />)}
        onChange={(text) => {
          if (author !== null) {
            describeCard(boardId, card.id, text, author);
          }
        }}
      />
      {entrance ? (
        <Section title="Handlers" count={model?.type === "entrance" ? model.targets.length : undefined}>
          <ul className="inspector-list">
            {model?.type === "entrance"
              ? model.targets.map((target) => (
                  <li key={target.position} className={`inspector-row static ${target.resolution}`}>
                    <span className={`inspector-name code-name ${target.resolution}`}>{target.name ?? target.text}</span>
                    {target.resolution === "resolved" && target.target !== null ? (
                      isOnBoard(boardId, target.target) ? (
                        <span className="inspector-dot" aria-label="On the board">●</span>
                      ) : (
                        <button
                          type="button"
                          className="inspector-action"
                          onClick={() => {
                            if (target.target !== null) {
                              followCall({ boardId, fromCardId: card.id, fromLabel: name, targetKey: target.target, targetLabel: target.name ?? target.text });
                            }
                          }}
                        >
                          + Open
                        </button>
                      )
                    ) : (
                      <span className="inspector-note">{target.module ?? target.note ?? target.resolution}</span>
                    )}
                  </li>
                ))
              : null}
          </ul>
        </Section>
      ) : (
        <Calls boardId={boardId} card={card} blockKey={key} name={name} />
      )}
      {!entrance ? (
        <Section title="Groups" count={groups.length}>
          {groups.length === 0 ? <p className="inspector-hint">Drag down the line numbers to group statements.</p> : null}
          <ul className="inspector-list">
            {groups.map((group) => (
              <GroupEditor key={group.id} boardId={boardId} group={group} author={author} issues={itemsFor(health, { group: group.id })} />
            ))}
          </ul>
          <button
            type="button"
            className="inspector-action"
            onClick={() => {
              openBoardDialog({ kind: "group", boardId, cardId: card.id });
            }}
          >
            Make a group…
          </button>
        </Section>
      ) : null}
      <div className="inspector-footer">
        <button
          type="button"
          className="inspector-action danger"
          onClick={() => {
            removeCardFromBoard(boardId, card.id);
          }}
        >
          Remove from board
        </button>
      </div>
    </div>
  );
}
