/**
 * Selected for a card: its name and location (a link that opens its source, as View code does), its
 * description (edited here or on the card), then for a block Called by and Reached from (up to 3 each,
 * then "N more · Show callers"), and its groups. A type card says where the type is used ("Used in N
 * places", a popover of the places), instead of calls for a type declaration. For an entrance, its
 * handlers.
 */

import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { cardQuery, showCallers } from "../../canvas";
import { UsedIn } from "../../code/UsedIn";
import { shortPath } from "../../code/text";
import { declarationWord, isTypeKind } from "../../code/typeKinds";
import { TextButton } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { Card, CardView, ProjectId } from "../../ipc/generated";
import { useAuthor } from "../../state/author";
import {
  addBlock,
  cardKey,
  describeCard,
  entranceTitle,
  findCardByKey,
  findPage,
  focusCard,
  getOpenBoard,
  itemsFor,
  keyDisplayName,
  openEntrance,
  select,
  useOpenBoard,
  type PageTarget,
} from "../../state/boards";
import { viewCode } from "../viewCode";
import { AuthoredField } from "./AuthoredField";
import { RelationRow } from "./Relations";

/** How many callers and entrances Selected lists before "N more". */
const RELATIONS_SHOWN = 3;

/** The focus target of the selected card's description. */
const DESCRIPTION_FIELD = "selected.description";

/** Brings a definition into view: its card when the page has one, else a new card. */
function showDefinition(target: PageTarget, key: string): void {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const existing = page ? findCardByKey(page, key) : undefined;
  if (existing) {
    focusCard(target, existing.id);
  } else {
    addBlock(target, key);
  }
}

function BlockRelations({ target, project, card }: { readonly target: PageTarget; readonly project: ProjectId; readonly card: Card }) {
  const relations = useEngineQuery("relations", { project, key: cardKey(card), callersLimit: RELATIONS_SHOWN });
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
          key={`${caller.source}:${caller.kind}`}
          {...(index === 0 ? { label: "Called by" } : {})}
          title={`${shortPath(caller.path)}:${caller.line}`}
          onActivate={() => {
            showDefinition(target, caller.owner);
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
            void openEntrance(target, entrance.key);
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

function EntranceHandlers({ model }: { readonly model: Extract<CardView, { type: "entrance" }> }) {
  return (
    <div className="selected-relations">
      {model.targets.map((handler, index) => (
        <RelationRow key={handler.position} {...(index === 0 ? { label: "Handlers" } : {})}>
          {handler.name ?? handler.text}
        </RelationRow>
      ))}
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

export function SelectedCard({ target, card }: { readonly target: PageTarget; readonly card: Card }) {
  const key = cardKey(card);
  const { project } = target.board;
  const author = useAuthor();
  const query = useQuery(cardQuery(project, key));
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
        : "";
  const disabledReason = held ? "Held for review: accept or reconnect it on the card first." : author === null ? "Loading who you are…" : null;
  return (
    <>
      <div className="selected-identity">
        <h3 className="selected-name">{name}</h3>
        {location && model !== undefined ? (
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
      {model?.type === "block" && isTypeKind(model.header.kind) ? <TypeUses project={project} typeKey={key} /> : null}
      {model?.type === "block" && declarationWord(model.header.kind) === undefined ? (
        <BlockRelations target={target} project={project} card={card} />
      ) : null}
      {model?.type === "entrance" ? <EntranceHandlers model={model} /> : null}
      <CardGroups target={target} card={card} />
    </>
  );
}
