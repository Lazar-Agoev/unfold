// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../ipc/client";
import { engineKey } from "../ipc/queries";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { projectRow, workspaceView } from "../test/workspace";
import { boardLink, copyBoardLink, copyLink } from "./links";

const toast = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../components/Toast", () => toast);

const P = "p-orders0000";
const TARGET = { board: { project: P, id: ".unfold/boards/order-placement.json" }, page: "page-1" };

let engine: FakeEngine;

beforeEach(() => {
  toast.showToast.mockClear();
  queryClient.clear();
  engine = installFakeEngine();
});

afterEach(() => {
  engine.uninstall();
});

describe("boardLink", () => {
  it("names the project by its folder name, then the board, page, and item", () => {
    expect(boardLink({ project: "ts-orders", board: ".unfold/boards/order-placement.json", page: "page-1", item: "create-order" })).toBe(
      "unfold://ts-orders/.unfold/boards/order-placement.json?page=page-1&item=create-order",
    );
    expect(boardLink({ project: "ts-orders", board: ".unfold/boards/b.json" })).toBe("unfold://ts-orders/.unfold/boards/b.json");
  });

  it("encodes each part, keeping the board's slashes", () => {
    expect(boardLink({ project: "my shop/web", board: ".unfold/boards/café.json", page: "page 1", item: "a&b" })).toBe(
      "unfold://my%20shop%2Fweb/.unfold/boards/caf%C3%A9.json?page=page%201&item=a%26b",
    );
  });
});

describe("copyLink", () => {
  it("copies a link to the board, the page, or an item with the project's folder name", async () => {
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "ts-orders")]));
    await copyBoardLink(TARGET.board);
    await copyLink(TARGET);
    await copyLink(TARGET, "create-order");
    expect(engine.windowCalls).toEqual([
      { command: "copy_to_clipboard", args: { text: "unfold://ts-orders/.unfold/boards/order-placement.json" } },
      { command: "copy_to_clipboard", args: { text: "unfold://ts-orders/.unfold/boards/order-placement.json?page=page-1" } },
      { command: "copy_to_clipboard", args: { text: "unfold://ts-orders/.unfold/boards/order-placement.json?page=page-1&item=create-order" } },
    ]);
    expect(toast.showToast).toHaveBeenCalledWith({ title: "Copied the link", body: "Paste it into an agent's chat to point at this." });
  });

  it("names the project by its id when another project's folder has the same name", async () => {
    const feature = "p-feature000";
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "ts-orders"), projectRow(feature, "ts-orders"), projectRow("p-payments00", "payments")]));
    await copyLink(TARGET);
    await copyLink({ ...TARGET, board: { ...TARGET.board, project: feature } });
    await copyBoardLink({ project: "p-payments00", id: ".unfold/boards/refunds.json" });
    expect(engine.windowCalls.map((call) => call.args)).toEqual([
      { text: "unfold://p-orders0000/.unfold/boards/order-placement.json?page=page-1" },
      { text: "unfold://p-feature000/.unfold/boards/order-placement.json?page=page-1" },
      { text: "unfold://payments/.unfold/boards/refunds.json" },
    ]);
  });

  it("says why when the board's project isn't in the workspace", async () => {
    await copyLink(TARGET);
    expect(engine.windowCalls).toEqual([]);
    expect(toast.showToast).toHaveBeenCalledWith(expect.objectContaining({ title: "Couldn't copy the link", tone: "danger" }));
  });
});
