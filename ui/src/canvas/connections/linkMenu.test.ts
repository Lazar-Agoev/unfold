// @vitest-environment jsdom
import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { queryClient } from "../../ipc/client";
import type { BoardDocument, Connection } from "../../ipc/generated";
import { engineKey } from "../../ipc/queries";
import { useFocusTarget } from "../../lib/focus-targets";
import * as store from "../../state/boards/store";
import { useLayout } from "../../state/layout";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../test/workspace";
import { LINK_DESCRIPTION_FIELD, LINK_LABEL_FIELD, kindEntries, linkMenuEntries } from "./linkMenu";

const toast = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../../components/Toast", () => toast);

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const AUTHOR = "person:maya";
const CALL = "shop/service.py#place_order/call:client.post";

let engine: FakeEngine;
let refuse: string | null;

function open(links: Connection[]): void {
  store.adoptBoardState(
    boardState(P, ID, boardDocument([page("page-1", [{ id: "place", ref: { block: "shop/service.py#place_order" }, at: [0, 0] }, { id: "save", ref: { block: "shop/store.py#save_order" }, at: [600, 0] }], { links })])),
  );
}

function link(): Connection | undefined {
  return store.getOpenBoard(REF)?.document.pages[0]?.links[0];
}

/** Picks entry `id` of connection `linkId`'s Kind ▸. */
function choose(linkId: string, id: string): void {
  const entry = kindEntries(PAGE, linkId, AUTHOR).find((item) => item.id === id);
  if (entry?.kind === "separator" || entry?.kind === "heading" || !entry?.onSelect) {
    throw new Error(`no ${id} entry`);
  }
  entry.onSelect();
}

beforeEach(() => {
  store.resetBoards();
  refuse = null;
  toast.showToast.mockClear();
  engine = installFakeEngine({
    handlers: {
      save_board: ({ board }) => {
        if (refuse !== null) {
          // Rejections are plain {code, message} data, as from the real preload.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "board_problem", message: refuse };
        }
        return boardState(P, ID, boardDocument(board.pages as BoardDocument["pages"]));
      },
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("Kind ▸", () => {
  it("offers calls only on a connection that leaves from a call, and other only once it has a label", () => {
    open([{ id: "drawn", kind: "http", from: "place", to: "save", author: AUTHOR }]);
    const drawn = kindEntries(PAGE, "drawn", AUTHOR);
    expect(drawn.map((entry) => entry.id)).toEqual(["kind-http", "kind-queue", "kind-reads", "kind-writes", "kind-relates", "kind-other"]);
    expect(drawn.find((entry) => entry.id === "kind-other")).toMatchObject({ disabled: true });

    open([{ id: "traced", kind: "http", from: "place", to: "save", step: CALL, label: { text: "Posts", author: AUTHOR, signature: "" }, author: "agent:claude" }]);
    const traced = kindEntries(PAGE, "traced", AUTHOR);
    expect(traced.map((entry) => entry.id)).toContain("kind-calls");
    expect(traced.find((entry) => entry.id === "kind-other")).toMatchObject({ disabled: false });
  });

  it("saves a new kind at once as one undo step, signed by the person", async () => {
    open([{ id: "drawn", kind: "http", from: "place", to: "save", author: "agent:claude" }]);
    choose("drawn", "kind-queue");
    await vi.waitFor(() => {
      expect(engine.callsOf("save_board")).toHaveLength(1);
    });
    await vi.waitFor(() => {
      expect(link()).toMatchObject({ kind: "queue", author: AUTHOR });
    });
    const board = store.getOpenBoard(REF);
    expect(board && store.isDirty(board)).toBe(false);
    expect(board?.history.past.map((step) => step.name)).toEqual(["Change kind"]);
    store.undo(REF);
    expect(link()?.kind).toBe("http");
  });

  it("leaves the connection as it was, with nothing unsaved, when the board refuses, and says why", async () => {
    open([{ id: "drawn", kind: "http", from: "place", to: "save", author: AUTHOR }]);
    refuse = "Can't connect place to save (queue): no.";
    choose("drawn", "kind-queue");
    await vi.waitFor(() => {
      expect(toast.showToast).toHaveBeenCalledWith({ title: "Couldn't change the kind", body: "Can't connect place to save (queue): no.", tone: "danger" });
    });
    const board = store.getOpenBoard(REF);
    expect(link()?.kind).toBe("http");
    expect(board && store.isDirty(board)).toBe(false);
    expect(board?.saveError).toBeNull();
    expect(board?.history.past).toEqual([]);
  });
});

describe("a connection's menu", () => {
  /** Picks entry `label` of connection `linkId`'s menu. */
  function pick(linkId: string, label: string): void {
    const entry = linkMenuEntries(PAGE, linkId, AUTHOR).find((item) => (item.kind === undefined || item.kind === "item") && item.label === label);
    if (!entry || (entry.kind !== undefined && entry.kind !== "item") || !entry.onSelect) {
      throw new Error(`no ${label} entry`);
    }
    entry.onSelect();
  }

  /** A field Selected shows under `key`, as its focus target. */
  function field(key: string): HTMLTextAreaElement {
    const element = document.createElement("textarea");
    document.body.append(element);
    onTestFinished(() => {
      element.remove();
    });
    renderHook(() => {
      useFocusTarget(key, { current: element });
    });
    return element;
  }

  const drawn: Connection = { id: "drawn", kind: "other", from: "place", to: "save", label: { text: "Posts", author: AUTHOR, signature: "" }, author: AUTHOR };

  it("lists Kind, Edit label, Describe…, Copy link, and Remove connection", () => {
    open([drawn]);
    expect(linkMenuEntries(PAGE, "drawn", AUTHOR).flatMap((entry) => (entry.kind === undefined || entry.kind === "item" ? [entry.label] : []))).toEqual(["Kind", "Edit label", "Describe…", "Copy link", "Remove connection"]);
  });

  it("Edit label selects it and focuses its label in Selected", () => {
    open([drawn]);
    useLayout.setState({ selectedCollapsed: true });
    const label = field(LINK_LABEL_FIELD);
    pick("drawn", "Edit label");
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "link", linkId: "drawn" });
    expect(useLayout.getState().selectedCollapsed).toBe(false);
    expect(document.activeElement).toBe(label);
  });

  it("Describe… selects it and focuses its description in Selected", () => {
    open([drawn]);
    const description = field(LINK_DESCRIPTION_FIELD);
    pick("drawn", "Describe…");
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "link", linkId: "drawn" });
    expect(document.activeElement).toBe(description);
  });

  it("Copy link copies a link to it", async () => {
    open([drawn]);
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "shop")]));
    pick("drawn", "Copy link");
    await vi.waitFor(() => {
      expect(engine.windowCalls).toEqual([{ command: "copy_to_clipboard", args: { text: "unfold://shop/.unfold/boards/b.json?page=page-1&item=drawn" } }]);
    });
  });

  it("Remove connection takes it off the page, as one undo step", () => {
    open([drawn]);
    pick("drawn", "Remove connection");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.links).toEqual([]);
    store.undo(REF);
    expect(link()).toEqual(drawn);
  });
});
