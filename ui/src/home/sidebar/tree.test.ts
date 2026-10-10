import { describe, expect, it } from "vitest";
import type { BoardList, BoardSummary } from "../../ipc";
import { folderKey } from "../../state/layout";
import { projectRow } from "../../test/workspace";
import { sidebarRows, treeRows, type SidebarRow } from "./tree";

const P = "p-shop000000";

function board(title: string, folder: string | null, place: BoardSummary["place"] = "published"): BoardSummary {
  return {
    project: P,
    id: `.unfold/boards/${title}.json`,
    place,
    name: title,
    title,
    folder,
    pages: 2,
    cards: 3,
    projects: [],
    changes: 0,
    updatedAtMs: 0,
    heldBy: null,
    label: null,
    error: null,
  };
}

const LIST: BoardList = {
  project: P,
  boards: [board("Payment webhooks", "Checkout"), board("Auth, end to end", null, "draft"), board("Order lifecycle", "Checkout"), board("Nightly", "Jobs/Cron")],
  emptyFolders: ["Archive", "Checkout/Old"],
};

function describeRows(rows: readonly SidebarRow[]): string[] {
  return rows.map((row) => {
    switch (row.kind) {
      case "project":
        return `project ${row.row.name}`;
      case "folder":
        return `${"  ".repeat(row.depth - 1)}${row.expanded ? "v" : ">"} ${row.name}${row.empty ? " (empty)" : ""}`;
      case "board":
        return `${"  ".repeat(row.depth - 1)}- ${row.board.title ?? ""}`;
      case "gap":
        return `gap ${row.height}`;
      case "note":
        return `note ${row.text}`;
    }
  });
}

describe("the sidebar tree", () => {
  it("lists folders first, then boards, each by name; collapsed folders hide their contents", () => {
    expect(describeRows(treeRows(P, LIST, new Set()))).toEqual(["> Archive (empty)", "> Checkout", "> Jobs", "- Auth, end to end"]);
  });

  it("opens the folders the window remembers", () => {
    const open = new Set([folderKey(P, "Checkout"), folderKey(P, "Jobs"), folderKey(P, "Jobs/Cron")]);
    expect(describeRows(treeRows(P, LIST, open))).toEqual([
      "> Archive (empty)",
      "v Checkout",
      "  > Old (empty)",
      "  - Order lifecycle",
      "  - Payment webhooks",
      "v Jobs",
      "  v Cron",
      "    - Nightly",
      "- Auth, end to end",
    ]);
  });

  it("puts each expanded project's tree under its row, padded as drawn", () => {
    const rows = sidebarRows(
      [projectRow(P, "storefront-api", { expanded: true }), projectRow("p-web", "storefront-web"), projectRow("p-new", "billing", { expanded: true })],
      new Map([
        [P, { list: LIST, error: null }],
        ["p-new", { list: { project: "p-new", boards: [], emptyFolders: [] }, error: null }],
      ]),
      new Set(),
    );
    expect(describeRows(rows)).toEqual([
      "project storefront-api",
      "gap 2",
      "> Archive (empty)",
      "> Checkout",
      "> Jobs",
      "- Auth, end to end",
      "gap 4",
      "project storefront-web",
      "project billing",
      "gap 2",
      "note No boards yet",
      "gap 4",
    ]);
  });

  it("shows nothing under a project whose boards are still loading", () => {
    const rows = sidebarRows([projectRow(P, "storefront-api", { expanded: true })], new Map(), new Set());
    expect(describeRows(rows)).toEqual(["project storefront-api"]);
  });
});
