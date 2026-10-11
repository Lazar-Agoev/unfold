import { describe, expect, it } from "vitest";
import type { CardView, Group, OutlineItem } from "../ipc/generated";
import { fixture } from "../test/engine";
import { stepGroups } from "./groups";

const card = fixture("card", "card_create_order") as Extract<CardView, { type: "block" }>;
if (card.code?.source.state !== "current") {
  throw new Error("the fixture's code should verify");
}
const outline = card.code.source.outline;
const FN = "src/routes/orders.ts#function:createOrder";
const IF = `${FN}/if[0]`;

function group(id: string, members: string[], collapsed = false): Group {
  return { id, card: "create-order", title: `Group ${id}`, members, collapsed };
}

function item(key: string, kind: OutlineItem["kind"], extra: Partial<OutlineItem> = {}): OutlineItem {
  return { key, kind, role: "body", startLine: 2, endLine: 2, start: 0, end: 1, groupable: true, ...extra };
}

describe("step groups", () => {
  it("keep the board's group and say whether a statement inside can return or throw", () => {
    const [queued, direct, branch] = stepGroups(
      [
        group("queue-it", [`${IF}/variable:job`, `${IF}/expression_statement[0]`, `${IF}/return[0]`], true),
        group("place-it", [`${FN}/variable:order`, `${FN}/expression_statement[0]`, `${FN}/expression_statement[1]`]),
        group("ask", [IF, `${FN}/variable:order`]),
      ],
      outline,
    );
    expect(queued).toEqual({
      id: "queue-it",
      title: "Group queue-it",
      members: [`${IF}/variable:job`, `${IF}/expression_statement[0]`, `${IF}/return[0]`],
      collapsed: true,
      returns: true,
      throws: false,
    });
    expect(direct).toMatchObject({ returns: false, throws: false });
    // The `if`'s fold summary says it returns.
    expect(branch).toMatchObject({ returns: true, throws: false });
  });

  it("read a one-line statement from what is written in it, never from a nested callable", () => {
    const callable: OutlineItem = { key: "a/variable:f/function:f", kind: "function", parent: 2, startLine: 2, endLine: 2, start: 0, end: 1, groupable: false };
    const lines: OutlineItem[] = [
      item("a/if[0]", "if"),
      item("a/if[0]/throw[0]", "throw", { role: "then", parent: 0 }),
      item("a/variable:f", "variable"),
      callable,
      item("a/variable:f/function:f/return[0]", "return", { parent: 3 }),
    ];
    const [guard, callback] = stepGroups([group("guard", ["a/if[0]"]), group("callback", ["a/variable:f"])], lines);
    expect(guard).toMatchObject({ returns: false, throws: true });
    expect(callback).toMatchObject({ returns: false, throws: false });
  });
});
