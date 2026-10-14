import { produce } from "immer";
import { describe, expect, it } from "vitest";
import type { Card, Frame, Page } from "../../ipc/generated";
import { page } from "../../test/workspace";
import { removeCard, removeItems, removeLink } from "./document";
import { addFrame, addToFrame, frameChain, frameItems, frameOf, frameRemoval, moveFrame, nestFrame, nestingRemoval, removeFrame, setFrameMembers, updateFrame } from "./frames";

const card = (id: string, at: [number, number] = [0, 0]): Card => ({ id, ref: { block: `src/a.ts#function:${id}` }, at });
const frame = (id: string, members: string[], extra: Partial<Frame> = {}): Frame => ({ id, title: id, members, look: "box", author: "agent:claude", ...extra });

/** A page with cards a–e, and these frames, links, and notes. */
function framed(overrides: Partial<Page> = {}): Page {
  return page("page-1", [card("a", [0, 0]), card("b", [600, 0]), card("c", [0, 400]), card("d", [600, 400]), card("e", [1200, 0])], overrides);
}

function onPage(start: Page, recipe: (draft: Page) => void): Page {
  return produce(start, recipe);
}

describe("frames on a page", () => {
  const nested = framed({ frames: [frame("zone", ["server", "e"], { look: "zone" }), frame("server", ["a", "checks"]), frame("checks", ["b"])] });

  it("know what holds an item, the frames around it, and everything inside one", () => {
    expect(frameOf(nested, "b")?.id).toBe("checks");
    expect(frameChain(nested, "b").map((item) => item.id)).toEqual(["checks", "server", "zone"]);
    expect(frameChain(nested, "d")).toEqual([]);
    expect(frameItems(nested, "zone")).toEqual(["server", "a", "checks", "b", "e"]);
  });

  it("adds a frame whose members leave the frames that held them, nested in place of the first member it takes", () => {
    const start = framed({ frames: [frame("server", ["a", "b", "c"])] });
    const result = onPage(start, (draft) => {
      expect(addFrame(draft, { title: "Request checks", members: ["b", "c"], look: "box", author: "agent:claude", parent: "server", collapsed: true, icon: "api" })).toBe("request-checks");
    });
    expect(result.frames).toEqual([
      frame("server", ["a", "request-checks"]),
      { id: "request-checks", title: "Request checks", members: ["b", "c"], look: "box", author: "agent:claude", icon: "api", collapsed: true },
    ]);
    // `collapsed` is written only when true.
    const open = onPage(start, (draft) => {
      addFrame(draft, { id: "x", title: "X", members: ["d"], look: "zone", author: "person:maya", collapsed: false });
    });
    expect(open.frames?.[1]).toEqual({ id: "x", title: "X", members: ["d"], look: "zone", author: "person:maya" });
  });

  it("sets members: new ones leave their frames, dropped ones go to the frame that holds it, and a frame left empty goes", () => {
    const start = framed({ frames: [frame("outer", ["inner", "e"]), frame("inner", ["a", "b"]), frame("other", ["c"])], links: [{ id: "to-other", from: "a", to: "other", kind: "calls", author: "person:lazar" }] });
    const result = onPage(start, (draft) => {
      setFrameMembers(draft, "inner", ["a", "c"]);
    });
    expect(result.frames).toEqual([frame("outer", ["inner", "b", "e"]), frame("inner", ["a", "c"])]);
    // The emptied frame took its link with it.
    expect(result.links).toEqual([]);
    expect(onPage(start, (draft) => {
      setFrameMembers(draft, "inner", ["a", "b"]);
    })).toBe(start);
  });

  it("nests and unnests a frame; leaving a frame empty never takes the new parent with it", () => {
    const start = framed({ frames: [frame("p", ["h"]), frame("h", ["f"]), frame("f", ["a"])] });
    const up = onPage(start, (draft) => {
      nestFrame(draft, "f", "p");
    });
    expect(up.frames).toEqual([frame("p", ["f"]), frame("f", ["a"])]);
    const out = onPage(start, (draft) => {
      nestFrame(draft, "f", null);
    });
    expect(out.frames).toEqual([frame("f", ["a"])]);
  });

  it("renames, describes, collapses, and changes how a frame looks", () => {
    const result = onPage(framed({ frames: [frame("server", ["a"], { collapsed: true })] }), (draft) => {
      updateFrame(draft, "server", { title: "Order API", author: "person:maya", description: { text: "Takes orders.", author: "person:maya" }, look: "zone", icon: "web", direction: "down", collapsed: false });
    });
    expect(result.frames?.[0]).toEqual({
      ...frame("server", ["a"]),
      title: "Order API",
      author: "person:maya",
      description: { text: "Takes orders.", author: "person:maya", signature: "" },
      look: "zone",
      icon: "web",
      direction: "down",
    });
    const cleared = onPage(result, (draft) => {
      updateFrame(draft, "server", { title: " ", description: { text: "", author: "person:maya" }, collapsed: true });
    });
    expect(cleared.frames?.[0]).toMatchObject({ title: "Order API", collapsed: true });
    expect(cleared.frames?.[0]?.description).toBeUndefined();
  });

  it("says what nesting a frame and removing one take with them: frames left empty, and what would dangle", () => {
    const start = framed({
      frames: [frame("p", ["h"]), frame("h", ["f"]), frame("f", ["a", "b"])],
      links: [
        { id: "to-h", from: "c", to: "h", kind: "calls", author: "person:lazar" },
        { id: "to-p", from: "c", to: "p", kind: "calls", author: "person:lazar" },
      ],
    });
    expect(nestingRemoval(start, "f", null)).toEqual(new Set(["h", "to-h", "p", "to-p"]));
    // The new parent holds it, so it stays.
    expect(nestingRemoval(start, "f", "p")).toEqual(new Set(["h", "to-h"]));
    expect(nestingRemoval(start, "f", "h")).toEqual(new Set());
    // Kept members take the frame's place, so the frame around it stays; else they go too.
    expect(frameRemoval(start, "h", true)).toEqual(new Set(["h", "to-h"]));
    expect(frameRemoval(start, "h", false)).toEqual(new Set(["h", "f", "a", "b", "to-h", "p", "to-p"]));
  });

  it("removes a frame keeping its members in its place; a note attached to it stays where it was", () => {
    const start = framed({
      frames: [frame("server", ["checks", "e"]), frame("checks", ["a", "b"])],
      links: [{ id: "to-checks", from: "e", to: "checks", kind: "calls", author: "person:lazar" }],
      notes: [{ id: "n", text: "Look here.", at: [10, 20], attach: "checks", author: "agent:claude" }],
    });
    let removed: string[] = [];
    const result = onPage(start, (draft) => {
      removed = removeFrame(draft, "checks");
    });
    expect(removed).toEqual(["to-checks", "checks"]);
    expect(result.frames).toEqual([frame("server", ["a", "b", "e"])]);
    expect(result.links).toEqual([]);
    // The checks frame's members' top-left is (0, 0).
    expect(result.notes).toEqual([{ id: "n", text: "Look here.", at: [10, 20], author: "agent:claude" }]);
  });

  it("removes a frame with everything in it when asked", () => {
    const start = framed({
      frames: [frame("server", ["checks", "e"]), frame("checks", ["a"])],
      combined: [{ id: "g", title: "G", members: ["k/if[0]"], collapsed: false, author: "agent:claude", signature: "" }],
      links: [{ id: "a-to-c", from: "a", to: "c", kind: "calls", author: "person:lazar" }],
    });
    const result = onPage(start, (draft) => {
      removeFrame(draft, "server", false);
    });
    expect(result.cards.map((item) => item.id)).toEqual(["b", "c", "d"]);
    // Combined steps apply page-wide, by key: removing a card keeps them.
    expect(result).toMatchObject({ combined: [{ id: "g" }], links: [] });
    // The file leaves out an empty list of frames.
    expect("frames" in result).toBe(false);
  });

  it("moves everything inside a frame by the same amount: cards, resource cards, and free notes", () => {
    const start = framed({
      frames: [frame("server", ["a", "inner", "orders", "free", "pinned"]), frame("inner", ["b"])],
      resources: [{ id: "orders", kind: "table", name: "orders", anchors: [], at: [100, 100], author: "agent:claude" }],
      notes: [
        { id: "free", text: "Free.", at: [50, 50], author: "agent:claude" },
        { id: "pinned", text: "Pinned.", at: [5, 5], attach: "a", author: "agent:claude" },
      ],
    });
    const result = onPage(start, (draft) => {
      moveFrame(draft, "server", 40, -10);
    });
    expect(result.cards.map((item) => [item.id, item.at])).toEqual([
      ["a", [40, -10]],
      ["b", [640, -10]],
      ["c", [0, 400]],
      ["d", [600, 400]],
      ["e", [1200, 0]],
    ]);
    expect(result.resources?.[0]?.at).toEqual([140, 90]);
    expect(result.notes?.map((note) => note.at)).toEqual([[90, 40], [5, 5]]);
  });

  it("adds items to a frame, each out of the frame that held it", () => {
    const result = onPage(framed({ frames: [frame("x", ["a"]), frame("y", ["b"])] }), (draft) => {
      addToFrame(draft, "x", ["b", "c"]);
    });
    expect(result.frames).toEqual([frame("x", ["a", "b", "c"])]);
  });
});

describe("the removal cascade", () => {
  it("takes a card out of its frame, removes a frame left empty (and a parent it empties), and the links to them", () => {
    const start = framed({
      frames: [frame("outer", ["inner"]), frame("inner", ["a"])],
      links: [
        { id: "to-outer", from: "b", to: "outer", kind: "calls", author: "person:lazar" },
        { id: "a-to-b", from: "a", to: "b", kind: "calls", author: "person:lazar" },
      ],
      notes: [{ id: "n", text: "Shared.", at: [10, 10], attach: "inner", author: "agent:claude" }],
    });
    let gone: string[] = [];
    const result = onPage(start, (draft) => {
      gone = removeItems(draft, ["a"]);
    });
    expect(gone).toEqual(["a", "to-outer", "a-to-b", "outer", "inner"]);
    expect("frames" in result).toBe(false);
    expect(result.links).toEqual([]);
    expect(result.notes).toEqual([{ id: "n", text: "Shared.", at: [10, 10], author: "agent:claude" }]);
    // removeCard is the same cascade.
    expect(onPage(start, (draft) => {
      removeCard(draft, "a");
    })).toEqual(result);
  });

  it("keeps a frame with members left, and detaches a link's notes at its midpoint", () => {
    const start = framed({
      frames: [frame("server", ["a", "b"])],
      links: [{ id: "a-to-b", from: "a", to: "b", kind: "calls", author: "person:lazar" }],
      notes: [{ id: "n", text: "Halfway.", at: [0, 30], attach: "a-to-b", author: "agent:claude" }],
    });
    const result = onPage(start, (draft) => {
      removeLink(draft, "a-to-b");
      removeCard(draft, "b");
    });
    expect(result.frames).toEqual([frame("server", ["a"])]);
    expect(result.notes).toEqual([{ id: "n", text: "Halfway.", at: [300, 30], author: "agent:claude" }]);
  });
});
