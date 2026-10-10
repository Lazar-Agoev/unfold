import { describe, expect, it } from "vitest";
import type { Action } from "../keyboard";
import { commandGroups, matchCommand } from "./commands";

function action(id: string, title: string, category: string, extra: Partial<Action> = {}): Action {
  return { id, title, category, shortcuts: [], order: 0, run: () => undefined, ...extra };
}

const MOVE_BACK = action("board.moveBackToDrafts", "Move back to drafts", "Board", { keywords: ["unpublish"] });

const ACTIONS: Action[] = [
  action("board.publish", "Publish", "Board", { icon: "publish", shortcuts: ["Mod+Shift+E"] }),
  MOVE_BACK,
  action("board.copyMermaid", "Copy Mermaid", "Board", { group: "Share" }),
  action("help.copyDiagnostics", "Copy Diagnostics", "Help"),
  action("tabs.home", "Home", "View"),
  action("peek.close", "Close Peek", "View", { showInAdd: false }),
  action("project.compile", "Compile", "Project"),
];

describe("matchCommand", () => {
  it("ranks a title prefix over a word start over a substring over letters in order", () => {
    const score = (query: string, title: string) => matchCommand(query, action("x", title, "Board"))?.score ?? -Infinity;
    expect(score("pub", "Publish")).toBeGreaterThan(score("pub", "Republish now"));
    expect(score("mer", "Copy Mermaid")).toBeGreaterThan(score("ish", "Publish"));
    expect(score("ish", "Publish")).toBeGreaterThan(score("pbs", "Publish"));
    expect(matchCommand("zzz", action("x", "Publish", "Board"))).toBeNull();
  });

  it("bolds the matched run, or the letters in order, and matches keywords without bold", () => {
    expect(matchCommand("pub", action("x", "Publish", "Board"))?.ranges).toEqual([[0, 3]]);
    expect(matchCommand("pbs", action("x", "Publish", "Board"))?.ranges).toEqual([
      [0, 1],
      [2, 3],
      [5, 6],
    ]);
    expect(matchCommand("unpub", MOVE_BACK)).toEqual({ score: 100, ranges: [] });
  });
});

describe("commandGroups", () => {
  it("without a query lists every enabled action shown in Add, grouped in ADD_GROUPS order", () => {
    const groups = commandGroups(ACTIONS, "", (id) => id !== "project.compile");
    expect(groups.map((group) => group.label)).toEqual(["Board", "Share", "View", "Help"]);
    expect(groups.flatMap((group) => group.items.map((item) => item.action.id))).toEqual([
      "board.moveBackToDrafts",
      "board.publish",
      "board.copyMermaid",
      "tabs.home",
      "help.copyDiagnostics",
    ]);
  });

  it("with a query ranks one Commands group and names a submenu's group before the title", () => {
    const [group] = commandGroups(ACTIONS, " copy ", () => true);
    expect(group?.label).toBe("Commands");
    expect(group?.items.map((item) => [item.action.title, item.prefix])).toEqual([
      ["Copy Mermaid", "Share"],
      ["Copy Diagnostics", null],
    ]);
    expect(commandGroups(ACTIONS, "nothing like it", () => true)).toEqual([]);
  });
});
