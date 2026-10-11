// @vitest-environment jsdom
import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { SaveBoardArgs } from "../../../ipc/generated";
import { engineKey } from "../../../ipc/queries";
import { installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../../test/workspace";
import * as store from "../store";
import { installAgentChannel, resetAgentChannel, setAgentChannelReady } from "./channel";
import { markWorking, resetWorking, useWorkingItems, workingText } from "./working";

const toast = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../../../components/Toast", () => toast);

const P = "p-orders0000";
const ID = ".unfold/local/drafts/b.json";
const REF = { project: P, id: ID };
const BOARD = { project: P, id: ID };
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  resetWorking();
  resetAgentChannel();
  toast.showToast.mockClear();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  engine = installFakeEngine({
    handlers: {
      save_board: (args: SaveBoardArgs) => boardState(P, ID, boardDocument(args.board.pages as never), { revision: "r2" }),
    },
  });
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }])])));
  installAgentChannel();
});

afterEach(() => {
  resetAgentChannel();
  engine.uninstall();
  store.resetBoards();
});

describe("the agent channel", () => {
  it("answers requests once the window's own state is restored, each with its window command", async () => {
    engine.emit({ type: "agent_read", requestId: 4, board: BOARD, page: "page-1" });
    engine.emit({ type: "agent_edit", requestId: 5, board: BOARD, page: "page-1", agent: CLAUDE, title: "Describe", ops: [{ op: "describe", items: [{ id: "a", text: "Does a." }] }] });
    await Promise.resolve();
    expect(engine.windowCalls).toEqual([]);

    setAgentChannelReady();
    await vi.waitFor(() => {
      expect(engine.windowCalls.map((entry) => entry.command).sort()).toEqual(["agent_edit_result", "agent_read_result"]);
    });
    const answers = new Map(engine.windowCalls.map((entry) => [entry.command, entry.args as { requestId: number; result: { ok: boolean } }]));
    expect(answers.get("agent_read_result")).toMatchObject({ requestId: 4, result: { ok: true, page: { id: "page-1" } } });
    expect(answers.get("agent_edit_result")).toMatchObject({ requestId: 5, result: { ok: true, items: [{ id: "a", status: "created" }] } });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.description?.author).toBe("agent:claude");
  });

  it("answers a background open, and shows an agent's finish", async () => {
    setAgentChannelReady();
    engine.emit({ type: "agent_open_board", requestId: 6, board: BOARD });
    await vi.waitFor(() => {
      expect(engine.windowCalls).toEqual([{ command: "agent_open_board_result", args: { requestId: 6, result: { ok: true } } }]);
    });
    engine.emit({ type: "agent_finish", board: BOARD, agent: CLAUDE, summary: "Done" });
    expect(toast.showToast).toHaveBeenCalledWith({ title: "Claude · Done" });
  });

  it("answers board requests: creating, editing pages, and showing", async () => {
    setAgentChannelReady();
    engine.emit({ type: "agent_edit_board", requestId: 7, board: BOARD, agent: CLAUDE, title: "Edit pages", ops: [{ op: "add_page", title: "More" }] });
    engine.emit({ type: "agent_show", requestId: 8, board: BOARD, item: "a" });
    engine.emit({ type: "agent_create_board", requestId: 9, project: P, copyOf: "other.json" });
    await vi.waitFor(() => {
      expect(engine.windowCalls.map((entry) => entry.command).sort()).toEqual(["agent_create_board_result", "agent_edit_board_result", "agent_show_result"]);
    });
    const answers = new Map(engine.windowCalls.map((entry) => [entry.command, entry.args]));
    expect(answers.get("agent_edit_board_result")).toMatchObject({ requestId: 7, result: { ok: true, results: [{ op: "add_page", status: "created", title: "More" }] } });
    expect(answers.get("agent_show_result")).toEqual({ requestId: 8, result: { ok: true, page: "page-1" } });
    expect(answers.get("agent_create_board_result")).toMatchObject({ requestId: 9, result: { ok: false, code: "board_not_open" } });
  });

  it("drops the working marks of a board the window no longer holds", () => {
    markWorking({ board: REF, page: "page-1" }, CLAUDE, ["a"]);
    const marks = () => [...renderHook(() => useWorkingItems({ board: REF, page: "page-1" })).result.current];
    expect(marks()).toEqual(["a"]);
    store.forgetBoard(REF);
    store.adoptBoardState(boardState(P, ID));
    expect(marks()).toEqual([]);
  });
});

describe("workingText", () => {
  it("names the agents working on a page", () => {
    expect(workingText(["Claude"])).toBe("Claude is working…");
    expect(workingText(["Claude", "Codex"])).toBe("Claude and Codex are working…");
    expect(workingText(["Claude", "Codex", "Zed"])).toBe("Claude, Codex and Zed are working…");
  });
});
