/**
 * Working marks: while an agent works on a page, the page's top bar says "Claude is working…", and
 * each item it wrote shows it on the canvas: a chip with the agent's name above the item, and the
 * words it wrote arriving (`canvas/writing.tsx`). An item's mark settles when the agent finishes
 * (`agent_finish`) or two minutes after its last write there: its words sharpen and its chip fades out
 * over 300 ms, then the mark is gone. Closing the board here drops its marks. Session state only:
 * never saved, never an undo step.
 */

import type { AgentIdentity } from "@unfold/desktop/agent-api";
import { useMemo } from "react";
import { create } from "zustand";
import { boardKey } from "../keys";
import type { BoardRef, PageId, PageTarget } from "../types";

/**
 * A field of an item an agent wrote: a card's, frame's, or resource card's description, a card's or
 * frame's title, a note's text, what a connection says (its chip's target stands for it), a resource
 * card's name, or the steps a card draws (its names, combined and proposed steps).
 */
export type WrittenField = "description" | "title" | "text" | "target" | "name" | "steps";

/** An item an agent changed, and the fields it wrote there (none when it only moved or added it). */
export interface ChangedItem {
  readonly id: string;
  readonly fields: readonly WrittenField[];
}

/** An agent's writing on one item: who, which fields, when its last write landed, and whether it is settling. */
export interface ItemWriting {
  /** The agent's name, as its chip says it ("Claude", "Codex", "Agent"). */
  readonly agent: string;
  readonly author: string;
  readonly fields: readonly WrittenField[];
  /** When the last write landed (ms, `Date.now()`). */
  readonly at: number;
  /** Finished, or two minutes without a write: its words sharpen and its chip fades out. */
  readonly settling: boolean;
}

/** How long an item's mark lasts after its last write, and how long it takes to fade out. */
export const WRITING_IDLE_MS = 120_000;
export const SETTLE_MS = 300;

/** One agent's work on a board: its name, and the pages it is working on. */
interface AgentWork {
  readonly name: string;
  readonly pages: readonly PageId[];
}

type Items = Readonly<Record<string, ItemWriting>>;

interface WorkingState {
  /** By `boardKey`, then by the agent's author (`agent:claude`). */
  readonly boards: Readonly<Record<string, Readonly<Record<string, AgentWork>>>>;
  /** By `boardKey`, then page, then item. */
  readonly items: Readonly<Record<string, Readonly<Record<PageId, Items>>>>;
}

const useWorking = create<WorkingState>()(() => ({ boards: {}, items: {} }));

const NO_ITEMS: ReadonlySet<string> = new Set();
const NO_WRITING: Items = {};

/** Timers by item (`board`, page, and item joined): when it settles, and when it is gone. */
const timers = new Map<string, ReturnType<typeof setTimeout>>();

function timerKey(key: string, page: PageId, item: string): string {
  return [key, page, item].join("\u0000");
}

function clearTimer(id: string): void {
  const timer = timers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    timers.delete(id);
  }
}

/** Sets one item's writing (null drops it). */
function setItem(key: string, page: PageId, item: string, writing: ItemWriting | null): void {
  useWorking.setState((state) => {
    const board = state.items[key] ?? {};
    const { [item]: _old, ...rest } = board[page] ?? {};
    const onPage = writing === null ? rest : { ...rest, [item]: writing };
    return { items: { ...state.items, [key]: { ...board, [page]: onPage } } };
  });
}

/** Drops an item's mark once it has faded. */
function scheduleGone(key: string, page: PageId, item: string): void {
  const id = timerKey(key, page, item);
  clearTimer(id);
  timers.set(
    id,
    setTimeout(() => {
      timers.delete(id);
      setItem(key, page, item, null);
    }, SETTLE_MS),
  );
}

/** Settles an item's mark: its words sharpen and its chip fades, then it is gone. */
function settle(key: string, page: PageId, item: string): void {
  const writing = useWorking.getState().items[key]?.[page]?.[item];
  if (writing && !writing.settling) {
    setItem(key, page, item, { ...writing, settling: true });
    scheduleGone(key, page, item);
  }
}

/** An agent changed `items` on a page: it is working there, and what it wrote shows it. */
export function markWorking(target: PageTarget, agent: AgentIdentity, items: readonly (string | ChangedItem)[], now = Date.now()): void {
  const key = boardKey(target.board);
  const page = target.page;
  useWorking.setState((state) => {
    const board = state.boards[key] ?? {};
    const work = board[agent.author];
    const pages = work?.pages.includes(page) === true ? work.pages : [...(work?.pages ?? []), page];
    const onBoard = state.items[key] ?? {};
    const onPage = { ...onBoard[page] };
    for (const entry of items) {
      const { id, fields } = typeof entry === "string" ? { id: entry, fields: [] } : entry;
      const old = onPage[id];
      const kept = old?.author === agent.author && !old.settling ? old.fields : [];
      onPage[id] = { agent: agent.name, author: agent.author, fields: [...new Set([...kept, ...fields])], at: now, settling: false };
    }
    return {
      boards: { ...state.boards, [key]: { ...board, [agent.author]: { name: agent.name, pages } } },
      items: { ...state.items, [key]: { ...onBoard, [page]: onPage } },
    };
  });
  for (const entry of items) {
    const item = typeof entry === "string" ? entry : entry.id;
    const id = timerKey(key, page, item);
    clearTimer(id);
    timers.set(
      id,
      setTimeout(() => {
        timers.delete(id);
        settle(key, page, item);
      }, WRITING_IDLE_MS),
    );
  }
}

/** An agent finished its work on a board: it stops working there, and what it wrote settles. */
export function finishWorking(ref: BoardRef, author: string): void {
  const key = boardKey(ref);
  const { boards, items } = useWorking.getState();
  if (boards[key]?.[author]) {
    const rest = Object.fromEntries(Object.entries(boards[key]).filter(([agent]) => agent !== author));
    useWorking.setState({ boards: Object.fromEntries(Object.entries({ ...boards, [key]: rest }).filter(([, agents]) => Object.keys(agents).length > 0)) });
  }
  for (const [page, onPage] of Object.entries(items[key] ?? {})) {
    for (const [item, writing] of Object.entries(onPage)) {
      if (writing.author === author) {
        settle(key, page, item);
      }
    }
  }
}

/** Drops the marks of boards this window no longer holds (`open`: the `boardKey`s it holds). */
export function pruneWorking(open: ReadonlySet<string>): void {
  const { boards, items } = useWorking.getState();
  if ([...Object.keys(boards), ...Object.keys(items)].some((key) => !open.has(key))) {
    for (const id of [...timers.keys()]) {
      if (!open.has(id.split("\u0000")[0] ?? "")) {
        clearTimer(id);
      }
    }
    useWorking.setState({
      boards: Object.fromEntries(Object.entries(boards).filter(([key]) => open.has(key))),
      items: Object.fromEntries(Object.entries(items).filter(([key]) => open.has(key))),
    });
  }
}

/** The names of the agents working on a page, kept current. */
export function useWorkingAgents(target: PageTarget): readonly string[] {
  const board = useWorking((state) => state.boards[boardKey(target.board)]);
  return useMemo(() => Object.values(board ?? {}).flatMap((work) => (work.pages.includes(target.page) ? [work.name] : [])), [board, target.page]);
}

/** The ids of the items agents changed on a page and are still working on, kept current. */
export function useWorkingItems(target: PageTarget): ReadonlySet<string> {
  const onPage = useWorking((state) => state.items[boardKey(target.board)]?.[target.page]);
  return useMemo(() => {
    const ids = Object.entries(onPage ?? {}).flatMap(([id, writing]) => (writing.settling ? [] : [id]));
    return ids.length > 0 ? new Set(ids) : NO_ITEMS;
  }, [onPage]);
}

/** What agents wrote on a page's items, settling marks included, kept current. */
export function usePageWriting(target: PageTarget): Items {
  return useWorking((state) => state.items[boardKey(target.board)]?.[target.page]) ?? NO_WRITING;
}

/** An agent's writing on one item of a page, while it shows (settling included); undefined otherwise. */
export function useItemWriting(target: PageTarget, item: string): ItemWriting | undefined {
  return useWorking((state) => state.items[boardKey(target.board)]?.[target.page]?.[item]);
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
  for (const id of [...timers.keys()]) {
    clearTimer(id);
  }
  useWorking.setState({ boards: {}, items: {} });
}
