/**
 * An agent's requests in this window (design §5.5): edits, reads of a held page, opening a board
 * without showing it, and finishing a piece of work.
 *
 * An edit loads what its operations read (card models, calls, callers), then applies them to the
 * session's working document with the same pure edits people's actions use, as one undo step named
 * "<agent> · <title>", every new or changed text signed with the agent's identity. It is saved at
 * once (`saveStep`) and becomes an undo step only once the engine accepts it, so a refused edit leaves
 * the board as it was and answers with the engine's code and message. The items it changed are marked
 * as the agent's work until it finishes.
 */

import type {
  AgentEditRequest,
  AgentEditResult,
  AgentFinish,
  AgentItemResult,
  AgentOp,
  AgentOpenRequest,
  AgentOpenResult,
  AgentReadRequest,
  AgentReadResult,
} from "@unfold/desktop/agent-api";
import { produce } from "immer";
import { isTypeKind, keyBlockKind } from "../../../code/typeKinds";
import { showToast } from "../../../components/Toast";
import type { Page } from "../../../ipc/generated";
import { toUnfoldError } from "../../../ipc/unfold";
import { cardKey, findPage } from "../document";
import { openBoard } from "../open";
import { getOpenBoard, isDirty, saveStep } from "../store";
import type { BoardRef } from "../types";
import { prepareAddCards } from "./cards";
import { applyConnect } from "./connections";
import { AgentRefusal, boardInfo, failure, type ApplyOp, type OpContext } from "./context";
import { AgentData } from "./data";
import { applyFrameCards } from "./frames";
import { applyGroupSteps } from "./groups";
import { applyArrange, prepareSetView } from "./layout";
import { applyAddNotes } from "./notes";
import { applyPropose } from "./proposals";
import { applyRemoveItems } from "./remove";
import { prepareAddResources } from "./resources";
import { applyDescribe, applyLabelSteps } from "./text";
import { finishWorking, markWorking } from "./working";

/** The board a request names, as this window's store keys it. */
function refOf(request: { readonly board: { readonly project: string; readonly id: string } }): BoardRef {
  return { project: request.board.project, id: request.board.id };
}

/**
 * An operation ready to apply: `add_cards` loads its cards and follows calls first, `set_view` loads
 * the callees and callbacks of the calls it expands in place (as `held`, the page now, holds the cards);
 * the rest apply as they are.
 */
async function prepare(op: AgentOp, held: Page, ctx: OpContext): Promise<ApplyOp> {
  switch (op.op) {
    case "add_cards":
      return prepareAddCards(op, ctx);
    case "set_view":
      return prepareSetView(op, held, ctx);
    case "arrange":
      return (page, run) => applyArrange(page, op, run);
    case "remove_items":
      return (page, run) => applyRemoveItems(page, op, run);
    case "group_steps":
      return (page, run) => applyGroupSteps(page, op, run);
    case "describe":
      return (page, run) => applyDescribe(page, op, run);
    case "label_steps":
      return (page, run) => applyLabelSteps(page, op, run);
    case "frame_cards":
      return (page, run) => applyFrameCards(page, op, run);
    case "add_resources":
      return prepareAddResources(op, ctx);
    case "connect":
      return (page, run) => applyConnect(page, op, run);
    case "add_notes":
      return (page, run) => applyAddNotes(page, op, run);
    case "propose":
      return (page, run) => applyPropose(page, op, run);
  }
}

/**
 * The items an edit changed, as its working marks outline them: cards, groups and their cards, links,
 * a label's card, frames, resource cards, notes, and proposed steps and their cards.
 */
function changedItems(items: readonly AgentItemResult[]): string[] {
  return items.flatMap((item) => {
    if (item.status !== "created" && item.status !== "updated") {
      return [];
    }
    switch (item.kind) {
      case "card":
      case "link":
      case "frame":
      case "resource":
      case "note":
        return [item.id];
      case "group":
      case "proposal":
        return [item.id, item.card];
      case "label":
        return [item.card];
    }
  });
}

/** Results as the save left them: a connection's channel, which the save fills in when it ends at an entrance. */
function asSaved(items: readonly AgentItemResult[], saved: Page): AgentItemResult[] {
  return items.map((item) => {
    const link = item.kind === "link" && item.status !== "removed" ? saved.links.find((entry) => entry.id === item.id) : undefined;
    return link?.channel !== undefined && item.kind === "link" ? { ...item, channel: link.channel } : item;
  });
}

/** Applies an agent's operations to a page of a board this window holds, as one undo step, saved at once. */
export async function applyAgentEdit(request: AgentEditRequest): Promise<AgentEditResult> {
  const ref = refOf(request);
  const board = getOpenBoard(ref);
  const page = board ? findPage(board.document, request.page) : undefined;
  if (!board) {
    return failure("board_not_open", "This window doesn't hold the board.");
  }
  if (!page) {
    return failure("page_not_found", `The board has no page with id “${request.page}”. list_boards lists its pages.`);
  }
  if (!request.agent.author.startsWith("agent:")) {
    return failure("invalid_op", "An agent's edit is signed with an agent's identity (agent:<name>).");
  }
  try {
    const base = {
      board: ref,
      agent: request.agent,
      data: new AgentData(),
      measured: board.pages[request.page]?.sizes ?? {},
    };
    // Every card's model (its size, its calls, its outline and Flow steps), each type card's definition
    // (its fields and type arrows), and the type of each table backed by one (its columns).
    await Promise.all([
      ...page.cards.map(async (card) => {
        const key = cardKey(card);
        return Promise.all([base.data.loadModel(ref.project, key), isTypeKind(keyBlockKind(key) ?? "") ? base.data.loadDefinition(ref.project, key) : null]);
      }),
      ...(page.resources ?? []).map((resource) => (resource.type !== undefined ? base.data.loadDefinition(ref.project, resource.type) : null)),
    ]);
    const applies: ApplyOp[] = [];
    for (const op of request.ops) {
      applies.push(await prepare(op, page, { ...base, warnings: [] }));
    }
    const outcome = await saveStep(ref, request.page, `${request.agent.name} · ${request.title}`, (document) => {
      const ctx: OpContext = { ...base, warnings: [] };
      let items: AgentItemResult[] = [];
      const next = produce(document, (draft) => {
        const target = draft.pages.find((entry) => entry.id === request.page);
        if (!target) {
          throw new AgentRefusal("page_not_found", `The board has no page with id “${request.page}” any more.`);
        }
        items = applies.flatMap((apply) => apply(target, ctx));
      });
      return { document: next, value: { items, warnings: ctx.warnings } };
    });
    if (!outcome.ok) {
      return failure(outcome.error.code, outcome.error.message);
    }
    const saved = findPage(outcome.state.document, request.page);
    if (!saved) {
      return failure("page_not_found", `The board has no page with id “${request.page}” any more.`);
    }
    const changed = changedItems(outcome.value.items);
    if (changed.length > 0) {
      markWorking({ board: ref, page: request.page }, request.agent, changed);
    }
    const { state } = outcome;
    return {
      ok: true,
      page: saved,
      board: boardInfo(state),
      items: asSaved(outcome.value.items, saved),
      warnings: outcome.value.warnings,
    };
  } catch (error) {
    if (error instanceof AgentRefusal) {
      return failure(error.code, error.message);
    }
    const { code, message } = toUnfoldError(error);
    return failure(code, message);
  }
}

/** A page of a board this window holds, as the window holds it (unsaved edits included). */
export function readAgentPage(request: AgentReadRequest): AgentReadResult {
  const board = getOpenBoard(refOf(request));
  const page: Page | undefined = board ? findPage(board.document, request.page) : undefined;
  if (!board) {
    return failure("board_not_open", "This window doesn't hold the board.");
  }
  if (!page) {
    return failure("page_not_found", `The board has no page with id “${request.page}”. list_boards lists its pages.`);
  }
  return {
    ok: true,
    page,
    board: {
      ...boardInfo(board),
      unsaved: isDirty(board),
    },
    sizes: board.pages[request.page]?.sizes ?? {},
  };
}

/** Opens a board for an agent without showing it. */
export async function openBoardForAgent(request: AgentOpenRequest): Promise<AgentOpenResult> {
  try {
    await openBoard(refOf(request), { background: true, ...(request.page !== undefined ? { page: request.page } : {}) });
    return { ok: true };
  } catch (error) {
    const { code, message } = toUnfoldError(error);
    return failure(code, message);
  }
}

/** An agent finished its work on a board: its working marks clear, and its summary shows as a toast. */
export function finishAgentWork(finish: AgentFinish): void {
  finishWorking(refOf(finish), finish.agent.author);
  const summary = finish.summary.trim();
  showToast({ title: summary.length > 0 ? `${finish.agent.name} · ${summary}` : `${finish.agent.name} finished` });
}
