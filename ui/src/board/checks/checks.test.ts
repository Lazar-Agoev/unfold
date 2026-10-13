import { describe, expect, it } from "vitest";
import type { Card, Frame, Group, Link, Note, Page, Resource } from "../../ipc/generated";
import { page as pageOf } from "../../test/workspace";
import { pageChecks } from "./checks";
import { frameBox, frameTree } from "./frames";
import type { Box, DrawnFlow, DrawnLine, PageGeometry } from "./geometry";
import { THRESHOLDS } from "./thresholds";

const AGENT = "agent:claude";
const PERSON = "person:ana";

function card(id: string, extra: Partial<Card> = {}): Card {
  return { id, ref: { block: `src/app.ts#function:${id}` }, at: [0, 0], ...extra };
}

function frame(id: string, members: string[], extra: Partial<Frame> = {}): Frame {
  return { id, title: id.toUpperCase(), members, look: "box", author: AGENT, description: { text: "What it does.", author: AGENT, signature: "" }, ...extra };
}

function resource(id: string, extra: Partial<Resource> = {}): Resource {
  return { id, kind: "table", name: id, anchors: [{ key: "src/db.ts#function:schema" }], at: [0, 0], author: AGENT, description: { text: "Rows.", author: AGENT, signature: "" }, ...extra };
}

function link(id: string, from: string, to: string, extra: Partial<Link> = {}): Link {
  return { id, from, to, ...extra };
}

function note(id: string, extra: Partial<Note> = {}): Note {
  return { id, text: "Mind this.", at: [0, 0], author: AGENT, ...extra };
}

function group(id: string, cardId: string, extra: Partial<Group> = {}): Group {
  return { id, card: cardId, title: id, members: [], collapsed: false, description: { text: "Steps.", author: AGENT, signature: "" }, ...extra };
}

/** The item without its description. */
function undescribed<T extends { readonly description?: unknown }>(item: T): T {
  const { description: _, ...rest } = item;
  return rest as T;
}

/** A page in Diagram view, so only the check under test finds anything. */
function page(cards: Card[], extra: Partial<Page> = {}): Page {
  return pageOf("page-1", cards, { defaultView: "diagram", ...extra });
}

function box(x: number, y: number, width = 200, height = 100): Box {
  return { x, y, width, height };
}

function line(id: string, from: string, to: string, points: [number, number][] = []): DrawnLine {
  return { id, from, to, points: points.map(([x, y]) => ({ x, y })) };
}

function drawn(boxes: Record<string, Box>, lines: DrawnLine[] = [], flows: Record<string, DrawnFlow> = {}): PageGeometry {
  return { boxes: new Map(Object.entries(boxes)), lines, flows: new Map(Object.entries(flows)) };
}

/** A row of cards far apart, joined by lines, so only the check under test finds anything. */
function quiet(ids: string[]): { cards: Card[]; boxes: Record<string, Box>; lines: DrawnLine[] } {
  const boxes = Object.fromEntries(ids.map((id, index) => [id, box(index * 400, 0)]));
  const lines = ids.slice(1).map((id, index) => line(`arrow-${index}`, ids[index] ?? "", id));
  return { cards: ids.map((id) => card(id)), boxes, lines };
}

const kinds = (checks: ReturnType<typeof pageChecks>) => checks.map((check) => check.kind);

describe("overlap", () => {
  it("finds cards on top of each other, naming both", () => {
    const checks = pageChecks(page([card("alpha"), card("beta"), card("gamma")]), drawn({ alpha: box(0, 0), beta: box(150, 50), gamma: box(800, 0) }, [line("l1", "alpha", "beta"), line("l2", "beta", "gamma")]));
    expect(checks).toEqual([
      { kind: "overlap", items: ["alpha", "beta"], message: "Two items overlap: alpha and beta.", suggestion: "Call arrange_page, or move them apart with its moves." },
    ]);
  });

  it("lets borders touch and a frame hold its members", () => {
    const cards = [card("alpha"), card("beta")];
    const frames = [frame("api", ["alpha", "beta"])];
    const geometry = drawn({ alpha: box(0, 0), beta: box(200, 0), api: box(-24, -40, 448, 164) }, [line("l1", "alpha", "beta")]);
    expect(kinds(pageChecks(page(cards, { frames }), geometry))).not.toContain("overlap");
  });

  it("finds a frame over a card it doesn't hold, measuring an undrawn frame around its members", () => {
    const cards = [card("alpha"), card("beta"), card("gamma")];
    const frames = [frame("api", ["alpha", "beta"])];
    // The frame isn't drawn: its box is its members' bounds plus a box's padding, which reaches gamma.
    const geometry = drawn({ alpha: box(0, 0), beta: box(220, 0), gamma: box(0, 110) }, [line("l1", "alpha", "beta"), line("l2", "beta", "gamma")]);
    const tree = frameTree(page(cards, { frames }));
    expect(frameBox(tree, geometry.boxes, "api")).toEqual(box(-16, -44, 452, 160));
    const [check] = pageChecks(page(cards, { frames }), geometry);
    expect(check?.kind).toBe("overlap");
    expect(check?.items).toEqual(["gamma", "api"]);
    expect(check?.message).toBe('Two items overlap: gamma and the frame "API".');
  });

  it("lets a note sit on what it is attached to, but not on anything else", () => {
    const cards = [card("alpha"), card("beta")];
    const notes = [note("n1", { attach: "alpha" }), note("n2")];
    const geometry = drawn({ alpha: box(0, 0), beta: box(600, 0), n1: box(20, 20, 120, 40), n2: box(620, 20, 120, 40) }, [line("l1", "alpha", "beta")]);
    const [check] = pageChecks(page(cards, { notes }), geometry);
    expect(check?.items).toEqual(["beta", "n2"]);
  });

  it("lets a frame hold a note attached to an item in it, nested frames included", () => {
    const cards = [card("alpha"), card("beta")];
    const frames = [frame("api", ["alpha"]), frame("system", ["api"])];
    const notes = [note("n1", { attach: "alpha" })];
    const geometry = drawn({ alpha: box(0, 0), beta: box(600, 0), n1: box(0, 112, 120, 40), api: box(-16, -44, 232, 212), system: box(-40, -100, 280, 292) }, [line("l1", "alpha", "beta")]);
    expect(kinds(pageChecks(page(cards, { frames, notes }), geometry))).not.toContain("overlap");
  });

  it("lets a frame hold a note on a line from or to an item in it", () => {
    const cards = [card("alpha"), card("beta")];
    const frames = [frame("api", ["alpha"])];
    const links = [link("l1", "alpha", "beta")];
    const notes = [note("n1", { attach: "l1" })];
    const geometry = drawn({ alpha: box(0, 0), beta: box(600, 0), n1: box(150, -40, 120, 40), api: box(-16, -44, 232, 160) }, [line("l1", "alpha", "beta")]);
    expect(kinds(pageChecks(page(cards, { frames, links, notes }), geometry))).not.toContain("overlap");
    // Over a card, it is still in the way.
    const over = drawn({ alpha: box(0, 0), beta: box(600, 0), n1: box(150, 20, 120, 40), api: box(-16, -44, 232, 160) }, [line("l1", "alpha", "beta")]);
    expect(pageChecks(page(cards, { frames, links, notes }), over)[0]?.items).toEqual(["alpha", "n1"]);
  });

  it("skips what a collapsed frame hides", () => {
    const cards = [card("alpha"), card("beta"), card("gamma")];
    const frames = [frame("api", ["alpha", "beta"], { collapsed: true })];
    const geometry = drawn({ alpha: box(0, 0), beta: box(10, 10), api: box(0, 0, 240, 80), gamma: box(600, 0) }, [line("l1", "api", "gamma")]);
    expect(kinds(pageChecks(page(cards, { frames }), geometry))).toEqual([]);
  });
});

describe("crossings", () => {
  /** Two vertical lines and two horizontal ones: four crossings, away from every end. */
  function grid(extra: DrawnLine[] = []): { page: Page; geometry: PageGeometry } {
    const ids = ["a", "b", "c", "d", "e", "f", "g", "h"];
    const boxes = Object.fromEntries(ids.map((id, index) => [id, box(index * 1000, 5000, 10, 10)]));
    const lines = [
      line("v1", "a", "b", [[100, 0], [100, 400]]),
      line("v2", "c", "d", [[300, 0], [300, 400]]),
      line("h1", "e", "f", [[0, 100], [400, 100]]),
      line("h2", "g", "h", [[0, 300], [400, 300]]),
      ...extra,
    ];
    return { page: page(ids.map((id) => card(id))), geometry: drawn(boxes, lines) };
  }

  it("finds lines that cross more often than the page allows", () => {
    const { page: drawnPage, geometry } = grid();
    const [check] = pageChecks(drawnPage, geometry);
    expect(check?.kind).toBe("crossings");
    expect(check?.items).toEqual(["a", "b", "c", "d", "e", "f", "g", "h"]);
    expect(check?.message).toMatch(/^Lines cross 4 times: the line from a to b crosses the line from e to f, /);
  });

  it("allows a few crossings, and doesn't count lines that share an item or cross at their ends", () => {
    const ids = ["a", "b", "c", "d", "e"];
    const boxes = Object.fromEntries(ids.map((id, index) => [id, box(index * 1000, 5000, 10, 10)]));
    const lines = [
      line("v1", "a", "b", [[100, 0], [100, 400]]),
      // Shares `a`: fans out from it.
      line("h1", "a", "c", [[0, 100], [400, 100]]),
      // Crosses v1 within the end slack of its own start.
      line("h2", "d", "e", [[95, 300], [400, 300]]),
    ];
    expect(kinds(pageChecks(page(ids.map((id) => card(id))), drawn(boxes, lines)))).toEqual([]);
    expect(THRESHOLDS.lineCrossings).toBe(3);
  });

  it("finds a line through a frame it neither starts nor ends in", () => {
    const cards = [card("alpha"), card("beta"), card("gamma"), card("delta")];
    const frames = [frame("api", ["beta"])];
    const geometry = drawn(
      { alpha: box(0, 0), beta: box(400, 300), gamma: box(800, 0), delta: box(400, 600), api: box(376, 276, 248, 148) },
      [
        line("through", "alpha", "gamma", [[200, 50], [500, 290], [800, 50]]),
        line("out", "beta", "delta", [[500, 400], [500, 600]]),
      ],
    );
    const checks = pageChecks(page(cards, { frames }), geometry);
    expect(checks).toEqual([
      {
        kind: "crossings",
        items: ["alpha", "gamma", "api"],
        message: '1 line crosses the frame "API" without starting or ending in it: the line from alpha to gamma.',
        suggestion: "Call arrange_page, or move the frame or the cards so the lines go around it.",
      },
    ]);
  });

  it("names a drawn link or a connection by its kind", () => {
    const cards = [card("alpha"), card("beta"), card("gamma")];
    const links = [link("call-api", "alpha", "gamma", { kind: "http", author: AGENT, anchors: [{ key: "src/app.ts#function:alpha" }] })];
    const frames = [frame("api", ["beta"])];
    const geometry = drawn({ alpha: box(0, 0), beta: box(400, 100), gamma: box(800, 0), api: box(376, 56, 248, 148) }, [line("call-api", "alpha", "gamma", [[200, 70], [800, 70]])]);
    const [check] = pageChecks(page(cards, { frames, links }), geometry);
    expect(check?.items).toEqual(["api", "call-api"]);
    expect(check?.message).toBe('1 line crosses the frame "API" without starting or ending in it: the http connection from alpha to gamma.');
  });

  it("finds a line under a card, resource card, or collapsed frame it neither starts nor ends at", () => {
    const cards = [card("alpha"), card("beta"), card("gamma"), card("delta"), card("hidden")];
    const resources = [resource("orders")];
    const frames = [frame("api", ["hidden"], { collapsed: true }), frame("zone", ["delta"])];
    const notes = [note("n")];
    const geometry = drawn(
      {
        alpha: box(0, 0),
        beta: box(400, 0),
        gamma: box(1200, 0),
        orders: box(800, 0),
        api: box(400, 400, 240, 80),
        delta: box(400, 800),
        zone: box(376, 756, 248, 168),
        n: box(800, 400, 120, 40),
      },
      [
        // Under beta and the table on its way to gamma.
        line("long", "alpha", "gamma", [[200, 50], [1200, 50]]),
        // Along beta's top edge, and through the expanded frame, the note, and the collapsed frame it ends at.
        line("edge", "alpha", "api", [[200, 0], [600, 0], [900, 420], [520, 440]]),
        // Out of its own card and into its own end.
        line("own", "beta", "orders", [[500, 50], [900, 50]]),
        line("framed", "zone", "gamma", [[300, 850], [1300, 100]]),
      ],
    );
    const checks = pageChecks(page(cards, { frames, resources, notes }), geometry);
    expect(checks).toEqual([
      {
        kind: "crossings",
        items: ["alpha", "beta", "gamma", "orders"],
        message: "1 line passes under a card it doesn't start or end at: the line from alpha to gamma passes under beta and the table \"orders\".",
        suggestion: "Call arrange_page, or move the cards off the line with its moves (rightOf, below), so the line can be seen end to end.",
      },
    ]);
    // Under a collapsed frame's node it doesn't end at, too.
    const past = [line("past", "alpha", "gamma", [[200, 450], [1300, 450]])];
    expect(pageChecks(page(cards, { frames, resources }), drawn({ alpha: box(0, 400), gamma: box(1300, 400), api: box(600, 400, 240, 80) }, past))[0]?.items).toEqual(["alpha", "gamma", "api"]);
  });
});

describe("density", () => {
  it("finds a page with more visible items than it reads well with, counting a collapsed frame once", () => {
    const ids = Array.from({ length: 33 }, (_, index) => `c${index}`);
    const { cards, boxes, lines } = quiet(ids);
    const crowded = pageChecks(page(cards), drawn(boxes, lines));
    expect(crowded.map((check) => [check.kind, check.items])).toEqual([["density", []]]);
    expect(crowded[0]?.message).toBe("The page shows 33 items (cards, resource cards, and collapsed frames); about 30 is the most a page reads well with.");

    const frames = [frame("folded", ["c0", "c1", "c2", "c3"], { collapsed: true })];
    expect(kinds(pageChecks(page(cards, { frames }), drawn(boxes, lines)))).not.toContain("density");
  });

  it("finds a frame with too many items, counting nested frames' items", () => {
    const ids = Array.from({ length: 14 }, (_, index) => `c${index}`);
    const { cards, boxes, lines } = quiet(ids);
    const frames = [frame("outer", ["inner", ...ids.slice(0, 7)]), frame("inner", ids.slice(7))];
    const [check] = pageChecks(page(cards, { frames }), drawn(boxes, lines));
    expect(check).toMatchObject({ kind: "density", items: ["outer"], message: 'The frame "OUTER" shows 14 items; about 12 is the most a frame reads well with.' });

    const collapsed = [frame("outer", ["inner", ...ids.slice(0, 7)]), frame("inner", ids.slice(7), { collapsed: true })];
    expect(kinds(pageChecks(page(cards, { frames: collapsed }), drawn(boxes, lines)))).not.toContain("density");
  });

  it("finds an expanded function card with too many steps, a legacy Code card too, and counts no compact card", () => {
    const { cards, boxes, lines } = quiet(["alpha", "beta", "gamma", "delta"]);
    const views = [{ view: "flow" }, { view: "flow" }, { view: "code" }, { view: "diagram" }] as const;
    const shown = cards.map((item, index) => ({ ...item, ...views[index] }));
    const flows = { alpha: { steps: 13 }, beta: { steps: 12 }, gamma: { steps: 14 }, delta: { steps: 20 } };
    const suggestion = "Combine with group_steps: 3 to 7 combined steps, collapsed: true for the ones the page isn't about; or set_view expanded: false.";
    expect(pageChecks(page(shown), drawn(boxes, lines, flows))).toEqual([
      { kind: "density", items: ["alpha"], message: "alpha shows 13 steps expanded; about 12 is the most an expanded card reads well with.", suggestion },
      { kind: "density", items: ["gamma"], message: "gamma shows 14 steps expanded; about 12 is the most an expanded card reads well with.", suggestion },
    ]);
    // The Code lens shows source, not steps.
    expect(kinds(pageChecks(page(shown, { defaultView: "code" }), drawn(boxes, lines, flows)))).not.toContain("density");
  });
});

describe("words", () => {
  it("finds frames, step groups, and resource cards without descriptions, in page order", () => {
    const { cards, boxes, lines } = quiet(["alpha", "beta"]);
    const extra: Partial<Page> = {
      frames: [undescribed(frame("api", ["alpha", "beta"]))],
      groups: [group("checks", "alpha"), undescribed(group("reply", "alpha", { title: "Reply" }))],
      resources: [undescribed(resource("orders"))],
    };
    const linked = [...lines, line("w", "alpha", "orders")];
    const checks = pageChecks(page(cards, extra), drawn({ ...boxes, orders: box(0, 400) }, linked));
    expect(checks).toEqual([
      {
        kind: "missingWords",
        items: ["api", "orders", "reply"],
        message: 'No description on the frame "API", the table "orders", and the step group "Reply".',
        suggestion: "describe each in one sentence: what it is and why, not how.",
      },
    ]);
  });
});

describe("evidence", () => {
  it("finds resource cards and connections that cite no code, but not actors, grounded steps, or drawn links", () => {
    const cards = [card("alpha"), card("beta")];
    const resources = [resource("orders", { anchors: [] }), resource("customer", { kind: "actor", name: "Customer", anchors: [] }), resource("jobs", { kind: "queue" })];
    const links = [
      link("drawn", "alpha", "beta"),
      link("post", "alpha", "beta", { kind: "http", author: PERSON, step: "src/app.ts#function:alpha/call[0]" }),
      link("save", "alpha", "orders", { kind: "writes", author: PERSON, step: "src/app.ts#function:alpha/call[1]" }),
      link("read", "beta", "orders", { kind: "reads", author: PERSON }),
      link("enqueue", "alpha", "jobs", { kind: "queue", author: AGENT, anchors: [{ key: "src/app.ts#function:alpha" }] }),
    ];
    const boxes = { alpha: box(0, 0), beta: box(400, 0), orders: box(0, 400), customer: box(400, 400), jobs: box(800, 400) };
    const lines = [...links.map((item) => line(item.id, item.from, item.to)), line("c", "customer", "alpha")];
    const checks = pageChecks(page(cards, { resources, links }), drawn(boxes, lines));
    expect(checks).toEqual([
      {
        kind: "evidence",
        items: ["orders", "post", "read"],
        message:
          'The table "orders", the http connection from alpha to beta, and the reads connection from beta to the table "orders" cite no code: nothing shows they are true, and nothing says when they go out of date.',
        suggestion: "Give each anchors (add_resources or connect with the same id, citing a block or an evidence file's lines); a person's are theirs to change, so ask first.",
      },
    ]);
  });
});

describe("strays", () => {
  it("finds cards and resource cards with no line, connection, or frame", () => {
    const cards = [card("alpha"), card("beta"), card("gamma"), card("delta")];
    const resources = [resource("orders")];
    const frames = [frame("api", ["delta"])];
    const boxes = { alpha: box(0, 0), beta: box(400, 0), gamma: box(800, 0), delta: box(1200, 0), orders: box(0, 400) };
    const checks = pageChecks(page(cards, { resources, frames }), drawn(boxes, [line("arrow", "alpha", "beta")]));
    expect(checks).toEqual([
      {
        kind: "strays",
        items: ["gamma", "orders"],
        message: 'gamma and the table "orders" have no line, connection, or frame to the rest of the page.',
        suggestion: "connect them, put them in a frame (frame_cards), or take them off the page (remove_items).",
      },
    ]);
  });

  it("leaves a page about one card alone, and counts links the canvas didn't draw", () => {
    expect(pageChecks(page([card("alpha")]), drawn({ alpha: box(0, 0) }))).toEqual([]);
    const cards = [card("alpha"), card("beta")];
    expect(pageChecks(page(cards, { links: [link("l", "alpha", "beta")] }), drawn({ alpha: box(0, 0), beta: box(400, 0) }))).toEqual([]);
  });
});

describe("pageChecks", () => {
  it("is deterministic and lists findings in the checklist's order", () => {
    const cards = [card("alpha", { view: "flow" }), card("beta"), card("gamma")];
    const resources = [resource("orders", { anchors: [] })];
    const geometry = drawn({ alpha: box(0, 0), beta: box(100, 0), gamma: box(800, 0), orders: box(0, 400) }, [line("l", "alpha", "beta")], { alpha: { steps: 14 } });
    const first = pageChecks(page(cards, { resources }), geometry);
    expect(kinds(first)).toEqual(["overlap", "density", "evidence", "strays"]);
    expect(pageChecks(page(cards, { resources }), geometry)).toEqual(first);
  });
});
