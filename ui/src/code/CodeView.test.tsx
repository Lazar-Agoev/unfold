// @vitest-environment jsdom
import "../test/dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type { CallLink, CardView, CodeText } from "../ipc/generated";
import { fixture } from "../test/engine";
import { CodeView, portAnchors } from "./CodeView";
import type { OutlineItem } from "../ipc/generated";
import { defaultFolds, foldHiding, groupConflict, groupForLines, groupSelection } from "./outline";
import { linkRanges, segmentRange, spanRanges } from "./segments";
import { linkNote } from "./text";

vi.mock("../state/boards/engine", () => ({ call: vi.fn(() => new Promise(() => undefined)), onEngineEvent: vi.fn() }));

const card = fixture("card", "card_create_order") as Extract<CardView, { type: "block" }>;
if (card.code?.source.state !== "current") {
  throw new Error("the fixture's code should verify");
}
const base: CodeText = card.code.source;
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
// The fixture predates call resolution; make `orderService.place` resolved for these tests.
const code: CodeText = {
  ...base,
  links: base.links.map((link): CallLink => (link.text === "orderService.place" ? { ...link, resolution: "resolved", target: PLACE } : link)),
};
const found = code.links.find((link) => link.text === "orderService.place");
if (!found) {
  throw new Error("the fixture calls orderService.place");
}
const placeLink: CallLink = found;
const IF = "src/routes/orders.ts#function:createOrder/if[0]";

/** What a reader sees: the text without hidden screen-reader descriptions. */
function visibleText(element: Element | null): string {
  const copy = element?.cloneNode(true) as Element | undefined;
  copy?.querySelectorAll("[hidden]").forEach((hidden) => {
    hidden.remove();
  });
  return copy?.textContent ?? "";
}

function wrap(node: ReactNode) {
  return <QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>;
}

describe("segments", () => {
  it("splits text into runs with one token class and at most one link", () => {
    const runs = segmentRange(0, 40, spanRanges(code.spans), linkRanges(code.links));
    expect(runs[0]).toEqual({ start: 0, end: 6, token: "keyword", link: null });
    expect(runs.map((run) => code.text.slice(run.start, run.end)).join("")).toBe(code.text.slice(0, 40));
    const linked = segmentRange(placeLink.span?.[0] ?? 0, placeLink.span?.[1] ?? 0, spanRanges(code.spans), linkRanges(code.links));
    expect(linked.every((run) => run.link === code.links.indexOf(placeLink))).toBe(true);
  });
});

describe("outline", () => {
  it("makes a group of consecutive sibling statements from a gutter selection", () => {
    expect(groupForLines(code.outline, 32, 34)).toEqual([
      "src/routes/orders.ts#function:createOrder/variable:order",
      "src/routes/orders.ts#function:createOrder/expression_statement[0]",
      "src/routes/orders.ts#function:createOrder/expression_statement[1]",
    ]);
    // Inside the `if` body: its statements, not the `if`.
    expect(groupForLines(code.outline, 28, 29)).toEqual([`${IF}/variable:job`, `${IF}/expression_statement[0]`]);
    // Starting on the `if` header takes the `if` itself.
    expect(groupForLines(code.outline, 27, 32)).toEqual([IF, "src/routes/orders.ts#function:createOrder/variable:order"]);
  });

  it("respects the model's default folds and finds the fold that hides an offset", () => {
    expect(defaultFolds(code.outline)).toEqual([]);
    const job = code.links.find((link) => link.text === "ordersQueue.add");
    expect(foldHiding(code.outline, new Set([IF]), job?.span?.[0] ?? 0)?.key).toBe(IF);
  });
});

describe("group validation", () => {
  // function f() {            1
  //   if (x) {                2
  //     a();                  3
  //     const g = () => 1;    4
  //   } else {                5
  //     b();                  6
  //     c();                  7
  //   }                       8
  //   d();                    9
  // }                         10
  const item = (key: string, startLine: number, endLine: number, fields: Partial<OutlineItem>): OutlineItem => ({
    key,
    kind: "expression_statement",
    startLine,
    endLine,
    start: startLine * 10,
    end: endLine * 10 + 9,
    groupable: true,
    ...fields,
  });
  const outline: OutlineItem[] = [
    item("f", 1, 10, { kind: "function", groupable: false }),
    item("f/if", 2, 8, { kind: "if", role: "body", parent: 0 }),
    item("f/if/a", 3, 3, { role: "then", parent: 1 }),
    item("f/if/g", 4, 4, { kind: "variable", role: "then", parent: 1, groupable: false }),
    item("f/if/b", 6, 6, { role: "else", parent: 1 }),
    item("f/if/c", 7, 7, { role: "else", parent: 1 }),
    item("f/d", 9, 9, { role: "body", parent: 0 }),
  ];

  it("groups consecutive statements of one body", () => {
    expect(groupSelection(outline, 6, 7)).toEqual({ members: ["f/if/b", "f/if/c"] });
    expect(groupSelection(outline, 9, 2)).toEqual({ members: ["f/if", "f/d"] });
  });

  it("refuses a group that crosses a body boundary, holds the card's header, or a nested definition", () => {
    expect(groupSelection(outline, 3, 6)).toEqual({ error: expect.stringContaining("can't cross a body boundary") as unknown });
    expect(groupSelection(outline, 1, 1)).toEqual({ error: expect.stringContaining("not the card's own header") as unknown });
    expect(groupSelection(outline, 3, 4)).toEqual({ error: expect.stringContaining("nested definition") as unknown });
    expect(groupForLines(outline, 3, 6)).toEqual([]);
  });

  it("refuses a group that overlaps or nests with another", () => {
    const existing = [{ title: "Branch", members: ["f/if"] }];
    expect(groupConflict(outline, ["f/if/b", "f/if/c"], existing)).toContain('overlap the group "Branch"');
    expect(groupConflict(outline, ["f/d"], existing)).toBeNull();
  });
});

describe("Code view", () => {
  it("renders the exact source with syntax classes and every line number", () => {
    const { container } = render(wrap(<CodeView code={code} language="typescript" label="createOrder" folds={new Set()} />));
    expect(container.querySelectorAll(".code-row")).toHaveLength(code.lines.length);
    expect(container.querySelector(".tok-keyword")?.textContent).toBe("export");
    expect(container.querySelector(".code-view")?.textContent).toContain('const order = orderService.place(draft);');
    expect(container.querySelector(".code-block[data-block]")).not.toBeNull();
  });

  it("marks external calls, opened calls, and a calling line's step fill (02 §4.3)", () => {
    const ports = new Set([placeLink.source]);
    const { container } = render(
      wrap(<CodeView code={code} language="typescript" label="createOrder" folds={new Set()} ports={ports} renderPorts={() => null} onFollow={vi.fn()} />),
    );
    expect(container.querySelector(".code-link.external")?.textContent).toBe("req.header");
    expect(container.querySelector(".code-link.resolved.opened")?.textContent).toBe("orderService.place");
    expect(container.querySelector(`.code-row.has-port[data-line="${placeLink.line}"]`)).not.toBeNull();
    // Every row has its line number in the gutter, with no fold column before it.
    expect(container.querySelectorAll(".code-gutter .code-number")).toHaveLength(code.lines.length);
  });

  it("follows on click, expands inline on ⌥-click, and explains unresolved calls", () => {
    const onFollow = vi.fn();
    const onExpand = vi.fn();
    const { container } = render(wrap(<CodeView code={code} language="typescript" label="createOrder" folds={new Set()} onFollow={onFollow} onExpand={onExpand} />));
    const link = screen.getByRole("button", { name: "Follow orderService.place" });
    expect(link.textContent).toBe("orderService.place");
    fireEvent.click(link);
    expect(onFollow).toHaveBeenCalledWith(placeLink, link);
    fireEvent.click(link, { altKey: true });
    expect(onExpand).toHaveBeenCalledWith(placeLink);
    // The first external or unresolved call in the model (which one depends on the compiler's
    // current resolution, so the golden may change).
    const plain = code.links.find((item) => item.resolution !== "resolved");
    if (!plain) {
      throw new Error("the golden has an external or unresolved call");
    }
    const unresolved = container.querySelector(`.code-link.${plain.resolution}`);
    expect(unresolved?.textContent).toBe(plain.text);
    // The reason is the link's accessible description, kept outside the code's text.
    const described = unresolved?.getAttribute("aria-describedby") ?? "";
    const note = linkNote(plain) ?? "";
    expect(document.getElementById(described)?.textContent).toBe(note);
    expect(container.textContent).not.toContain(note);
  });

  it("folds a compound block into one row that keeps its header and calls, and moves its ports there", () => {
    const toggle = vi.fn();
    const job = code.links.find((link) => link.text === "ordersQueue.add")?.source ?? "";
    const ports = new Set([job, placeLink.source]);
    const anchors = portAnchors(code, ports, new Set([IF]), []);
    expect(anchors.get(`fold:${IF}`)).toEqual([job]);
    expect(anchors.get("line:32")).toEqual([placeLink.source]);
    const { container } = render(
      wrap(
        <CodeView
          code={code}
          language="typescript"
          label="createOrder"
          folds={new Set([IF])}
          onToggleFold={toggle}
          ports={ports}
          renderPorts={(ids) => ids.map((id) => <i key={id} data-port={id} />)}
        />,
      ),
    );
    const folded = container.querySelector(".code-row.folded");
    expect(visibleText(folded)).toContain('if (req.header("prefer") === "respond-async")');
    expect(folded?.textContent).toContain("4 lines · returns");
    expect(folded?.querySelector(`[data-port="${job}"]`)).not.toBeNull();
    expect(container.querySelectorAll(".code-row")).toHaveLength(code.lines.length - 4);
    fireEvent.click(screen.getByRole("button", { name: "Unfold 4 lines · returns" }));
    expect(toggle).toHaveBeenCalledWith(IF);
  });

  it("draws a board group on a magenta rail with its title", () => {
    const { container } = render(
      wrap(
        <CodeView
          code={code}
          language="typescript"
          label="createOrder"
          folds={new Set()}
          groups={[{ id: "sync", card: "c", title: "Place and acknowledge", collapsed: false, members: groupForLines(code.outline, 32, 34) }]}
        />,
      ),
    );
    const group = container.querySelector('.code-group[data-group="sync"]');
    expect(group?.querySelector(".code-group-title")?.textContent).toBe("Place and acknowledge");
    expect(group?.querySelectorAll(".code-row")).toHaveLength(3);
  });

  it("copies exactly the code: tooltips' descriptions are not part of a row's text", () => {
    const { container } = render(wrap(<CodeView code={code} language="typescript" label="createOrder" folds={new Set()} />));
    // A line holding an external or unresolved call, whose tooltip explains it.
    const plain = code.links.find((item) => item.resolution !== "resolved");
    const number = plain?.line ?? 0;
    const row = container.querySelector(`.code-row[data-line="${number}"] .code-text`);
    expect(row?.querySelector(`.code-link.${plain?.resolution ?? "unresolved"}`)).not.toBeNull();
    const line = code.lines.find((item) => item.number === number);
    const index = code.lines.findIndex((item) => item.number === number);
    if (!row || !line) {
      throw new Error("the line is drawn");
    }
    const expected = code.text.slice(line.start + (index === 0 ? 0 : Math.min(code.dedent, line.indent)), line.end);
    expect(row.textContent).toBe(expected);
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(row);
    selection?.removeAllRanges();
    selection?.addRange(range);
    expect(selection?.toString()).toBe(expected);
    // Every row together is the whole slice, line by line.
    const all = [...container.querySelectorAll(".code-row .code-text")].map((element) => element.textContent).join("\n");
    expect(all).toBe(code.lines.map((item, at) => code.text.slice(item.start + (at === 0 ? 0 : Math.min(code.dedent, item.indent)), item.end) || " ").join("\n"));
  });
});
