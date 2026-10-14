/**
 * Working marks: while an agent works on a page, the page's top bar says "Claude is
 * working…" and the items it changed have a soft outline, until the agent finishes (`agent_finish`)
 * or the board closes here. Session state only: never saved, never an undo step.
 */

import type { AgentIdentity } from "@unfold/desktop/agent-api";
import { useMemo } from "react";
import { create } from "zustand";
import { boardKey } from "../keys";
import type { BoardRef, PageId, PageTarget } from "../types";

/** One agent's work on a board: its name, and the items it changed on each page. */
interface AgentWork {
  readonly name: string;
  readonly pages: Readonly<Record<PageId, readonly string[]>>;
}

interface WorkingState {
  /** By `boardKey`, then by the agent's author (`agent:claude`). */
  readonly boards: Readonly<Record<string, Readonly<Record<string, AgentWork>>>>;
}

const useWorking = create<WorkingState>()(() => ({ boards: {} }));

const NO_ITEMS: ReadonlySet<string> = new Set();

/** An agent changed `items` on a page: it is working there, and they are outlined. */
export function markWorking(target: PageTarget, agent: AgentIdentity, items: readonly string[]): void {
  const key = boardKey(target.board);
  useWorking.setState((state) => {
    const board = state.boards[key] ?? {};
    const work = board[agent.author];
    const outlined = new Set([...(work?.pages[target.page] ?? []), ...items]);
    return {
      boards: {
        ...state.boards,
        [key]: { ...board, [agent.author]: { name: agent.name, pages: { ...work?.pages, [target.page]: [...outlined] } } },
      },
    };
  });
}

/** An agent finished its work on a board: its marks clear. */
export function finishWorking(ref: BoardRef, author: string): void {
  const key = boardKey(ref);
  useWorking.setState((state) => {
    const board = state.boards[key];
    if (!board?.[author]) {
      return state;
    }
    const rest = Object.fromEntries(Object.entries(board).filter(([agent]) => agent !== author));
    return { boards: Object.fromEntries(Object.entries({ ...state.boards, [key]: rest }).filter(([, agents]) => Object.keys(agents).length > 0)) };
  });
}

/** Drops the marks of boards this window no longer holds (`open`: the `boardKey`s it holds). */
export function pruneWorking(open: ReadonlySet<string>): void {
  const { boards } = useWorking.getState();
  if (Object.keys(boards).some((key) => !open.has(key))) {
    useWorking.setState({ boards: Object.fromEntries(Object.entries(boards).filter(([key]) => open.has(key))) });
  }
}

/** The names of the agents working on a page, kept current. */
export function useWorkingAgents(target: PageTarget): readonly string[] {
  const board = useWorking((state) => state.boards[boardKey(target.board)]);
  return useMemo(
    () => Object.values(board ?? {}).flatMap((work) => (work.pages[target.page] !== undefined ? [work.name] : [])),
    [board, target.page],
  );
}

/** The ids of the items agents changed on a page, kept current. */
export function useWorkingItems(target: PageTarget): ReadonlySet<string> {
  const board = useWorking((state) => state.boards[boardKey(target.board)]);
  return useMemo(() => {
    const items = Object.values(board ?? {}).flatMap((work) => work.pages[target.page] ?? []);
    return items.length > 0 ? new Set(items) : NO_ITEMS;
  }, [board, target.page]);
}

/** "Claude is working…", or "Claude and Codex are working…". */
export function workingText(names: readonly string[]): string {
  if (names.length < 2) {
    return `${names[0] ?? "An agent"} is working…`;
  }
  return `${names.slice(0, -1).join(", ")} and ${names.at(-1) ?? ""} are working…`;
}

/** Forgets every mark (tests). */
export function resetWorking(): void {
  useWorking.setState({ boards: {} });
}
