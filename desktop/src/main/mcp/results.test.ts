import { describe, expect, it } from "vitest";
import { cant, fitResult, header, MAX_RESULT_CHARS, nextLine, paged, textResult, ToolError } from "./results";

const CLAUDE = { author: "agent:claude", name: "Claude" } as const;

describe("result headers (design §5.6)", () => {
  it("name the project, board, page, and revision, and who is asking", () => {
    expect(header(CLAUDE, { project: "ts-orders", board: { title: "Order placement", draft: true }, page: "Placing an order", revision: "1a2b3c4d" })).toBe(
      "Unfold · ts-orders › Order placement (draft) › Placing an order · rev 1a2b3c4d · as Claude",
    );
    expect(header(CLAUDE, { project: "ts-orders", board: { title: "Orders", draft: false } })).toBe("Unfold · ts-orders › Orders · as Claude");
    expect(header(CLAUDE)).toBe("Unfold · as Claude");
  });
});

describe("compact results", () => {
  it("drop empty lines", () => {
    expect(textResult("a", null, "", false, undefined, "b")).toEqual({ content: [{ type: "text", text: "a\nb" }] });
  });

  it("stay under about 6,000 tokens, cut at a line", () => {
    const line = `${"x".repeat(99)}\n`;
    const long = textResult(line.repeat(300));
    const [block] = fitResult(long).content;
    const text = block?.type === "text" ? block.text : "";
    expect(text.length).toBeLessThan(MAX_RESULT_CHARS + 200);
    expect(text.split("\n").slice(0, -1).every((kept) => kept === "x".repeat(99))).toBe(true);
    expect(text).toMatch(/\n… Cut here to stay under about 6,000 tokens\. Ask for less: a narrower filter, or the next page\.$/);
    const short = textResult("short");
    expect(fitResult(short)).toEqual(short);
  });
});

describe("cursors", () => {
  const items = Array.from({ length: 25 }, (_, index) => index);

  it("page a list, and say how to get the next page", () => {
    const first = paged(items, undefined, 10);
    expect(first).toEqual({ items: items.slice(0, 10), total: 25, next: "10" });
    expect(nextLine(first, "list_entrances")).toBe('Showing 10 of 25. Next page: list_entrances with cursor "10".');
    const last = paged(items, "20", 10);
    expect(last).toEqual({ items: items.slice(20), total: 25 });
    expect(nextLine(last, "list_entrances")).toBeNull();
    expect(paged([], undefined, 10)).toEqual({ items: [], total: 0 });
  });

  it("refuse a cursor that isn't one", () => {
    for (const cursor of ["abc", "-1", "1.5", "25", "99"]) {
      expect(() => paged(items, cursor, 10)).toThrow(ToolError);
    }
  });
});

describe("errors that teach (design §7.8)", () => {
  it("say what failed, why, and what to do instead", () => {
    expect(cant("add the table “orders”", "the column “customer_id” doesn't appear in its anchors.", "Cite where the column is defined, or leave it out.").message).toBe(
      "Can't add the table “orders”: the column “customer_id” doesn't appear in its anchors. Cite where the column is defined, or leave it out.",
    );
  });
});
