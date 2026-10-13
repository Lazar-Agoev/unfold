/**
 * Selected for a card: its name and location (a link that opens its source, as View code does), its
 * description (edited here or on the card), then for a block Called by and Reached from (up to 3 each,
 * then "N more · Show callers"), Not opened (up to 5 calls not on the page, each "+ Open"), and its
 * groups. A type card says where the type is used ("Used in N places", a popover of the places),
 * instead of calls for a type declaration. For an entrance, its handlers. A card of a project that
 * isn't connected shows what the board knows about it.
 */

import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { cardQuery, showCallers } from "../../canvas";
import { UsedIn } from "../../code/TypeDefinition";
import { shortPath } from "../../code/text";
import { declarationWord, isTypeKind } from "../../code/typeKinds";
import { TextButton } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { Card, CardView, OutgoingCall, ProjectId } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import {
  addBlock,
  cardKey,
  describeCard,
  entranceTitle,
  findCardByRef,
  findPage,
  focusCard,
  followCall,
  getOpenBoard,
  itemsFor,
  keyDisplayName,
  openEntrance,
  pushFocusMove,
  select,
  useOpenBoard,
  type PageTarget,
} from "../../state/boards";
import { viewCode } from "../viewCode";
import { AuthoredField } from "./AuthoredField";
import { RelationRow } from "./Relations";

/** How many callers and entrances Selected lists before "N more", and how many unopened calls. */
const RELATIONS_SHOWN = 3;
const UNOPENED_SHOWN = 5;

/** The focus target of the selected card's description. */
const DESCRIPTION_FIELD = "selected.description";

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

/** Brings a definition of the card's project into view: its card when the page has one, else a new card. */
function showDefinition(target: PageTarget, project: ProjectId, card: Card, key: string, label: string): void {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const existing = page ? findCardByRef(page, key, card.ref.project) : undefined;
  if (existing) {
    pushFocusMove(target, [{ cardId: existing.id, label }]);
    focusCard(target, existing.id);
  } else {
    addBlock(target, project, key);
  }
}

function BlockRelations({ target, project, card, name }: { readonly target: PageTarget; readonly project: ProjectId; readonly card: Card; readonly name: string }) {
  const key = cardKey(card);
  const cards = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.cards);
  // This project's keys on the page, which turn calls to them into "on the page" (not "Not opened").
  const onPage = useMemo(
    () => [...new Set((cards ?? []).filter((other) => other.ref.project === card.ref.project).map(cardKey))].sort(),
    [cards, card.ref.project],
  );
  const relations = useEngineQuery("relations", { project, key, onPage, callersLimit: RELATIONS_SHOWN });
  if (relations.isPending) {
    return <p className="selected-note">Loading…</p>;
  }
  if (relations.isError) {
    return <p className="selected-note">{relations.error.message}</p>;
  }
  const { callers, callersTotal, reachedFrom, outgoing, targetNames } = relations.data;
  const unopened = uniqueTargets(outgoing.openable).slice(0, UNOPENED_SHOWN);
  const nameOf = (call: OutgoingCall) => (call.target !== undefined ? (targetNames[call.target] ?? keyDisplayName(call.target)) : call.text);
  return (
    <div className="selected-relations">
      {callers.map((caller, index) => (
        <RelationRow
          key={`${caller.source}:${caller.kind}`}
          {...(index === 0 ? { label: "Called by" } : {})}
          title={`${shortPath(caller.path)}:${caller.line}`}
          onActivate={() => {
            showDefinition(target, project, card, caller.owner, caller.ownerName);
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
                void showCallers(target, card.id);
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
          title={entrance.path}
          onActivate={() => {
            void openEntrance(target, project, entrance.key);
          }}
        >
          {entrance.method !== null && entrance.trigger !== null ? `${entrance.method} ${entrance.trigger}` : (entrance.trigger ?? entrance.title)}
        </RelationRow>
      ))}
      {reachedFrom.length > RELATIONS_SHOWN ? (
        <RelationRow>
          <span className="relation-more">{reachedFrom.length - RELATIONS_SHOWN} more</span>
        </RelationRow>
      ) : null}
      {unopened.map((call, index) => (
        <RelationRow
          key={call.target ?? `${call.source}:${call.seq}`}
          {...(index === 0 ? { label: "Not opened" } : {})}
          action={
            <TextButton
              className="relation-action"
              aria-label={`Open ${nameOf(call)} on the page`}
              onClick={() => {
                if (call.target !== undefined) {
                  followCall({ target, fromCardId: card.id, fromLabel: name, targetKey: call.target, targetLabel: nameOf(call), line: call.line });
                }
              }}
            >
              + Open
            </TextButton>
          }
        >
          {nameOf(call)}
        </RelationRow>
      ))}
    </div>
  );
}

/** Where a type card's type is written: "Used in N places", opening the places. */
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

function EntranceHandlers({ target, card, model, name }: { readonly target: PageTarget; readonly card: Card; readonly model: Extract<CardView, { type: "entrance" }>; readonly name: string }) {
  const cards = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.cards);
  return (
    <div className="selected-relations">
      {model.targets.map((handler, index) => {
        const key = handler.resolution === "resolved" ? handler.target : null;
        const onPage = key !== null && (cards ?? []).some((other) => other.ref.project === card.ref.project && cardKey(other) === key);
        return (
          <RelationRow
            key={handler.position}
            {...(index === 0 ? { label: "Handlers" } : {})}
            action={
              key !== null && !onPage ? (
                <TextButton
                  className="relation-action"
                  onClick={() => {
                    followCall({ target, fromCardId: card.id, fromLabel: name, targetKey: key, targetLabel: handler.name ?? handler.text });
                  }}
                >
                  + Open
                </TextButton>
              ) : undefined
            }
          >
            {handler.name ?? handler.text}
          </RelationRow>
        );
      })}
    </div>
  );
}

function CardGroups({ target, card }: { readonly target: PageTarget; readonly card: Card }) {
  const allGroups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const groups = useMemo(() => (allGroups ?? []).filter((group) => group.card === card.id), [allGroups, card.id]);
  if (groups.length === 0) {
    return null;
  }
  return (
    <div className="selected-relations">
      {groups.map((group, index) => (
        <RelationRow
          key={group.id}
          {...(index === 0 ? { label: "Groups" } : {})}
          onActivate={() => {
            select(target, { kind: "group", groupId: group.id });
          }}
        >
          <span className="relation-group">{group.title}</span>
        </RelationRow>
      ))}
    </div>
  );
}

export function SelectedCard({ target, card, project }: { readonly target: PageTarget; readonly card: Card; readonly project: ProjectId | null }) {
  const key = cardKey(card);
  const author = useAuthor();
  const query = useQuery({ ...cardQuery(project ?? "", key), enabled: project !== null });
  const health = useOpenBoard(target.board, (board) => board.health);
  const issues = itemsFor(health, target.page, { card: card.id });
  const held = issues.some((item) => item.kind === "review");
  const model = query.data;
  const name =
    model?.type === "block" ? model.header.qualifiedName : model?.type === "entrance" ? entranceTitle(model) : keyDisplayName(key);
  const location =
    model?.type === "block"
      ? `${model.header.path}:${model.header.startLine}`
      : model?.type === "entrance"
        ? `${model.evidence.path}${model.evidence.startLine !== null ? `:${model.evidence.startLine}` : ""}`
        : project === null
          ? `From ${card.ref.project ?? ""} · not connected`
          : "";
  const disabledReason = project === null
    ? "Connect its project to describe it."
    : held
      ? "Held for review: accept or reconnect it on the card first."
      : author === null
        ? "Loading who you are…"
        : null;
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{name}</h3>
        {location && model !== undefined && project !== null ? (
          <p className="selected-location">
            <button
              type="button"
              className="selected-link"
              onClick={() => {
                viewCode(target, { kind: "card", cardId: card.id, key: null });
              }}
            >
              {location}
            </button>
          </p>
        ) : location ? (
          <p className="selected-location">{location}</p>
        ) : null}
      </div>
      <AuthoredField
        label="Description"
        value={card.description}
        placeholder="What this does, in a sentence…"
        focusKey={DESCRIPTION_FIELD}
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
      {project !== null && model?.type === "block" && isTypeKind(model.header.kind) ? <TypeUses project={project} typeKey={key} /> : null}
      {project !== null && model?.type === "block" && declarationWord(model.header.kind) === undefined ? (
        <BlockRelations target={target} project={project} card={card} name={name} />
      ) : null}
      {model?.type === "entrance" ? <EntranceHandlers target={target} card={card} model={model} name={name} /> : null}
      <CardGroups target={target} card={card} />
    </>
  );
}
