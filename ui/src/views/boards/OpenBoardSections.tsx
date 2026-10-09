/**
 * The open board in the Boards view (architecture §11.3, reader.md "Outline"): ON THIS BOARD, its
 * cards in follow order indented by call depth (click focuses the card); "N Flow nodes still show
 * code names", which steps through them; GROUPS; and what needs attention, each with its repairs.
 */

import { ChevronRight, ListCollapse } from "lucide-react";
import { useMemo, useState } from "react";
import { showInInspector } from "../../canvas/cards/cardMenu";
import { openBoardDialog } from "../../boards/dialogs";
import { IconButton } from "../../components";
import type { BoardOutline, Card } from "../../ipc/generated";
import { useRegisterActions } from "../../keyboard";
import { focusCard, foldAllCards } from "../../state/boards/api";
import { cardKey, keyDisplayName } from "../../state/boards/document";
import { kindWord, targetName } from "../../state/boards/health";
import { getBoard, pushFocusMove, selectBlock, useBoards } from "../../state/boards/store";
import { activateTab, boardTabId } from "../../state/tabs";

const NO_CARDS: readonly Card[] = [];
const NO_OUTLINE: BoardOutline = { rows: [], unlabeled: [] };

/** The outline's rows for the cards on the board now: cards added since the last save go last. */
export function outlineRows(outline: BoardOutline, cards: readonly Card[]): { card: Card; depth: number }[] {
  const byId = new Map(cards.map((card) => [card.id, card]));
  const rows: { card: Card; depth: number }[] = [];
  const seen = new Set<string>();
  for (const row of outline.rows) {
    const card = byId.get(row.card);
    if (card && !seen.has(card.id)) {
      seen.add(card.id);
      rows.push({ card, depth: row.depth });
    }
  }
  for (const card of cards) {
    if (!seen.has(card.id)) {
      rows.push({ card, depth: 0 });
    }
  }
  return rows;
}

/** Shows a card of the board: its tab, then the card itself, as a focus move (Back returns). */
function goToCard(boardId: string, cardId: string): void {
  const card = getBoard(boardId)?.document.cards.find((item) => item.id === cardId);
  if (!card) {
    return;
  }
  activateTab(boardTabId(boardId));
  pushFocusMove(boardId, [{ cardId, label: keyDisplayName(cardKey(card)) }]);
  focusCard(boardId, cardId);
}

function useUnlabeled(boardId: string) {
  const outline = useBoards((state) => state.boards[boardId]?.outline ?? NO_OUTLINE);
  const labels = useBoards((state) => state.boards[boardId]?.document.labels);
  const cards = useBoards((state) => state.boards[boardId]?.document.cards ?? NO_CARDS);
  return useMemo(() => {
    const onBoard = new Set(cards.map((card) => card.id));
    // Labels written since the last save are already labeled.
    return outline.unlabeled.filter((item) => onBoard.has(item.card) && labels?.[item.key] === undefined);
  }, [outline, labels, cards]);
}

/** Selects the next Flow node still showing a code name and opens the inspector on its label. */
function useStepper(boardId: string) {
  const unlabeled = useUnlabeled(boardId);
  const [step, setStep] = useState(0);
  const next = () => {
    if (unlabeled.length === 0) {
      return;
    }
    const index = step % unlabeled.length;
    const item = unlabeled[index];
    setStep(index + 1);
    if (!item) {
      return;
    }
    goToCard(boardId, item.card);
    selectBlock(boardId, item.card, item.key);
    showInInspector("inspector.label");
  };
  return { unlabeled, next };
}

export function OpenBoardSections({ boardId }: { readonly boardId: string }) {
  const cards = useBoards((state) => state.boards[boardId]?.document.cards ?? NO_CARDS);
  const outline = useBoards((state) => state.boards[boardId]?.outline ?? NO_OUTLINE);
  const groups = useBoards((state) => state.boards[boardId]?.document.groups);
  const health = useBoards((state) => state.boards[boardId]?.health);
  const document = useBoards((state) => state.boards[boardId]?.document);
  const selected = useBoards((state) => state.boards[boardId]?.view.selectedCardId ?? null);
  const rows = useMemo(() => outlineRows(outline, cards), [outline, cards]);
  const stepper = useStepper(boardId);
  useRegisterActions([
    {
      id: "board.nextUnlabeled",
      title: "Next Flow Node Showing a Code Name",
      category: "Board",
      enabled: () => stepper.unlabeled.length > 0,
      run: stepper.next,
    },
  ]);

  if (!document) {
    return null;
  }
  const count = stepper.unlabeled.length;
  return (
    <>
      <section className="boards-section" aria-labelledby="on-this-board">
        <div className="view-header">
          <h3 className="view-title" id="on-this-board">
            On this board
          </h3>
          <div className="view-actions">
            <IconButton icon={ListCollapse} label="Fold All Cards" size="small" disabled={cards.length === 0} onClick={() => { foldAllCards(boardId, true); }} />
          </div>
        </div>
        {rows.length === 0 ? <p className="view-hint">No cards yet.</p> : null}
        <ul className="boards-outline">
          {rows.map(({ card, depth }) => (
            <li key={card.id}>
              <button
                type="button"
                className={`boards-outline-row${card.id === selected ? " selected" : ""}`}
                style={{ paddingLeft: 14 + depth * 14 }}
                aria-current={card.id === selected ? "true" : undefined}
                onClick={() => {
                  goToCard(boardId, card.id);
                }}
              >
                <span className="boards-outline-name">{keyDisplayName(cardKey(card))}</span>
                {card.id === selected ? <span className="boards-outline-dot" aria-hidden="true">●</span> : null}
              </button>
            </li>
          ))}
        </ul>
        {count > 0 ? (
          <button type="button" className="boards-unlabeled" onClick={stepper.next}>
            <span>
              {count} Flow {count === 1 ? "node still shows a code name" : "nodes still show code names"}
            </span>
            <span className="boards-unlabeled-next">
              Next
              <ChevronRight size={12} aria-hidden="true" />
            </span>
          </button>
        ) : null}
      </section>
      {(groups?.length ?? 0) > 0 ? (
        <section className="boards-section" aria-labelledby="board-groups">
          <div className="view-header">
            <h3 className="view-title" id="board-groups">
              Groups
            </h3>
          </div>
          <ul className="boards-outline">
            {(groups ?? []).map((group) => {
              const card = cards.find((item) => item.id === group.card);
              return (
                <li key={group.id}>
                  <button
                    type="button"
                    className="boards-outline-row"
                    onClick={() => {
                      goToCard(boardId, group.card);
                    }}
                  >
                    <span className="boards-group-title">{group.title}</span>
                    <span className="boards-outline-detail">{card ? keyDisplayName(cardKey(card)) : group.card}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}
      {(health?.items.length ?? 0) > 0 ? (
        <section className="boards-section" aria-labelledby="board-health">
          <div className="view-header">
            <h3 className="view-title" id="board-health">
              Needs attention
            </h3>
          </div>
          <ul className="boards-outline">
            {(health?.items ?? []).map((item, index) => (
              <li key={`${index}:${item.message}`}>
                <button
                  type="button"
                  className={`boards-outline-row boards-issue kind-${item.kind}`}
                  title={item.message}
                  onClick={() => {
                    openBoardDialog({ kind: "repair", boardId, item });
                  }}
                >
                  <span className="boards-issue-kind">{kindWord(item.kind)}</span>
                  <span className="boards-outline-name">{targetName(item.target, document)}</span>
                  <span className="boards-outline-detail">Repair…</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </>
  );
}
