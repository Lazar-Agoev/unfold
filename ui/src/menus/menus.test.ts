import { describe, expect, it, vi } from "vitest";
import type * as Components from "../components";
import type { MenuEntry } from "../components";
import type * as Commands from "../ipc/commands";
import type { BoardList, BoardSummary, ProjectState } from "../ipc";
import { projectRow } from "../test/workspace";
import { boardMenuFor, projectFolders, type BoardFacts } from "./boardMenu";
import { projectMenuEntries } from "./projectMenu";

const { call, showToast } = vi.hoisted(() => ({ call: vi.fn(), showToast: vi.fn() }));
vi.mock("../ipc/commands", async (original) => ({ ...(await original<typeof Commands>()), call }));
vi.mock("../components", async (original) => ({ ...(await original<typeof Components>()), showToast }));

type Item = Exclude<MenuEntry, { readonly kind: "separator" } | { readonly kind: "heading" }>;

function items(entries: readonly MenuEntry[]): Item[] {
  return entries.filter((entry): entry is Item => entry.kind !== "separator" && entry.kind !== "heading");
}

function labels(entries: readonly MenuEntry[]): string[] {
  return entries.map((entry) => (entry.kind === "separator" ? "—" : "label" in entry ? entry.label : ""));
}

function enabled(entries: readonly MenuEntry[]): string[] {
  return items(entries)
    .filter((entry) => !entry.disabled)
    .map((entry) => entry.label);
}

const REF = { project: "p-1", id: ".unfold/boards/order-lifecycle.json" };

function summary(overrides: Partial<BoardSummary> = {}): BoardSummary {
  return {
    project: "p-1",
    id: REF.id,
    place: "published",
    name: "order-lifecycle",
    title: "Order lifecycle",
    folder: "Checkout",
    pages: 2,
    cards: 6,
    projects: [],
    changes: 0,
    updatedAtMs: 0,
    label: null,
    error: null,
    ...overrides,
  };
}

const OPEN: BoardFacts["open"] = { title: "Order lifecycle", place: "published", changes: 2, page: "page-1" };

describe("the board menu", () => {
  it("is the drawn menu for a published board with changes (12 §2.2)", () => {
    const entries = boardMenuFor(REF, { summary: summary({ changes: 2 }), folders: ["Checkout"], open: OPEN });
    expect(labels(entries)).toEqual([
      "Rename…",
      "Move to folder",
      "Duplicate",
      "—",
      "Share",
      "Fold all cards",
      "Unfold all cards",
      "—",
      "Discard changes…",
      "Move back to drafts",
      "—",
      "Delete…",
    ]);
    const share = items(entries).find((entry) => entry.id === "share");
    expect(share?.submenu?.map((entry) => ("label" in entry ? entry.label : ""))).toEqual(["Export Mermaid…", "Copy Mermaid", "Export PNG…"]);
    expect(items(entries).find((entry) => entry.id === "delete")?.danger).toBe(true);
  });

  it("drops Discard without changes and both file items for a draft", () => {
    const published = boardMenuFor(REF, { summary: summary(), folders: [], open: OPEN });
    expect(labels(published)).toContain("Move back to drafts");
    expect(labels(published)).not.toContain("Discard changes…");
    const draft = boardMenuFor(REF, { summary: summary({ place: "draft" }), folders: [], open: { ...OPEN, place: "draft", changes: 0 } });
    expect(labels(draft)).not.toContain("Move back to drafts");
    expect(labels(draft).filter((label) => label === "—")).toHaveLength(2);
  });

  it("disables what needs a page for a board that isn't open", () => {
    const entries = boardMenuFor(REF, { summary: summary(), folders: [], open: null });
    expect(enabled(entries)).toEqual(["Rename…", "Move to folder", "Duplicate", "Move back to drafts", "Delete…"]);
  });

  it("lists the project's folders with the board's own checked", () => {
    const entries = boardMenuFor(REF, { summary: summary(), folders: ["Billing", "Checkout"], open: OPEN });
    const move = items(entries).find((entry) => entry.id === "move")?.submenu ?? [];
    expect(labels(move)).toEqual(["Top level", "—", "Billing", "Checkout", "—", "New folder…"]);
    expect(items(move).filter((entry) => entry.checked).map((entry) => entry.label)).toEqual(["Checkout"]);
  });

  it("gathers folders from boards (with the folders above them) and empty folders", () => {
    const list: BoardList = {
      project: "p-1",
      boards: [summary({ folder: "Checkout/Payments" }), summary({ id: "b", folder: null }), summary({ id: "c", folder: "billing 10" })],
      emptyFolders: ["billing 2"],
    };
    expect(projectFolders(list)).toEqual(["billing 2", "billing 10", "Checkout", "Checkout/Payments"]);
  });
});

function projectMenu(state: ProjectState): string[] {
  return enabled(projectMenuEntries(projectRow("p-1", "shop", { state })));
}

describe("the project menu", () => {
  it("is the drawn menu: Compile again, Cancel, Open unfold.toml, Remove from workspace…", () => {
    const entries = projectMenuEntries(projectRow("p-1", "shop"));
    expect(labels(entries)).toEqual(["Compile again", "Cancel", "—", "Open unfold.toml", "—", "Remove from workspace…"]);
    expect(items(entries).find((entry) => entry.label === "Remove from workspace…")?.danger).toBe(true);
  });

  it("enables Cancel only while compiling", () => {
    expect(projectMenu({ kind: "current" })).toEqual(["Compile again", "Open unfold.toml", "Remove from workspace…"]);
    expect(projectMenu({ kind: "compiling", done: 1, total: 2 })).toEqual(["Cancel", "Open unfold.toml", "Remove from workspace…"]);
    expect(projectMenu({ kind: "resting", last: { kind: "current" }, atMs: 0 })).toEqual(["Compile again", "Open unfold.toml", "Remove from workspace…"]);
    expect(projectMenu({ kind: "unavailable", reason: "gone" })).toEqual(["Remove from workspace…"]);
  });

  it("opens unfold.toml in the editor, and shows why when it can't", async () => {
    const open = items(projectMenuEntries(projectRow("p-1", "shop"))).find((entry) => entry.label === "Open unfold.toml");
    call.mockResolvedValueOnce(null);
    open?.onSelect?.();
    await vi.waitFor(() => {
      expect(call).toHaveBeenCalledWith("open_in_editor", { project: "p-1", path: "unfold.toml" });
    });
    call.mockRejectedValueOnce({ code: "editor_not_found", message: "The editor command `zed` wasn't found." });
    open?.onSelect?.();
    await vi.waitFor(() => {
      expect(showToast).toHaveBeenCalledWith({ title: "Couldn't open the file", body: "The editor command `zed` wasn't found.", tone: "danger" });
    });
  });
});
