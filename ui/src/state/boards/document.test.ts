import { produce } from "immer";
import { describe, expect, it } from "vitest";
import type { BoardDocument, Card, Page } from "../../ipc/generated";
import { boardDocument, page } from "../../test/workspace";
import {
  addCard,
  addPage,
  deletePage,
  findCombined,
  isOpen,
  movePage,
  nextPageId,
  openPaths,
  removeCard,
  removeCombined,
  setCardDescription,
  setCardView,
  setName,
  toggleOpen,
  uniqueId,
  updateCombined,
} from "./document";
import { setLinkLabel } from "./connections";
import { addProposal, removeProposal, updateProposal } from "./proposals";

function onPage(recipe: (draft: Page) => void, start: Page = page("page-1")): Page {
  return produce(start, recipe);
}

function onBoard(recipe: (draft: BoardDocument) => void, start: BoardDocument = boardDocument()): BoardDocument {
  return produce(start, recipe);
}

describe("card, combined step, and connection ids", () => {
  it("are made from the card's name, unique together within the page, slugged, and at most 100 characters", () => {
    const result = onPage((draft) => {
      addCard(draft, { ref: { block: "key-1" }, name: "createOrder", at: [0.4, 9.6] });
      addCard(draft, { ref: { block: "key-1" }, name: "createOrder", at: [0, 0] });
      draft.combined.push({ id: "create-order-3", title: "x", members: ["k"], collapsed: false, author: "agent:claude", signature: "" });
      addCard(draft, { ref: { block: "key-2" }, name: "createOrder", at: [0, 0] });
    });
    expect(result.cards.map((card) => card.id)).toEqual(["create-order", "create-order-2", "create-order-4"]);
    expect(result.cards[0]?.at).toEqual([0, 10]);
    expect(uniqueId(result, "x".repeat(300)).length).toBeLessThanOrEqual(100);
    expect(uniqueId(result, "***")).toBe("card");
  });

  it("share one space with the page's frames, resources, and notes", () => {
    const start = page("page-1", [], {
      frames: [{ id: "orders", title: "Orders", members: ["orders-2"], look: "box", author: "agent:claude" }],
      resources: [{ id: "orders-2", kind: "table", name: "orders", anchors: [], at: [0, 0], author: "agent:claude" }],
      notes: [{ id: "orders-3", text: "Kept in memory.", at: [0, 0], author: "agent:claude" }],
    });
    expect(uniqueId(start, "orders")).toBe("orders-4");
  });
});

describe("combined steps", () => {
  const start = page("page-1", [], { combined: [{ id: "g", title: "Checks", members: ["k1", "k2"], collapsed: false, author: "agent:claude", signature: "" }] });

  it("are renamed, described, collapsed, and opened; a new title is its writer's", () => {
    const same = onPage((draft) => { updateCombined(draft, "g", { title: "Checks", author: "person:maya", collapsed: true }); }, start);
    expect(same.combined[0]).toMatchObject({ title: "Checks", collapsed: true, author: "agent:claude" });
    const renamed = onPage((draft) => { updateCombined(draft, "g", { title: "Guard", author: "person:maya", description: { text: "Stops early.", author: "person:maya" } }); }, start);
    expect(findCombined(renamed, "g")).toEqual({
      id: "g",
      title: "Guard",
      members: ["k1", "k2"],
      collapsed: false,
      author: "person:maya",
      signature: "",
      description: { text: "Stops early.", author: "person:maya", signature: "" },
    });
    expect(onPage((draft) => { updateCombined(draft, "g", { description: { text: " ", author: "person:maya" } }); }, renamed).combined[0]).not.toHaveProperty("description");
  });

  it("apply page-wide by key: removing a card keeps them, and removing one draws its blocks again", () => {
    const withCard = page("page-1", [{ id: "a", ref: { block: "key-a" }, at: [0, 0] }], { combined: start.combined });
    expect(onPage((draft) => { removeCard(draft, "a"); }, withCard).combined).toHaveLength(1);
    expect(onPage((draft) => { removeCombined(draft, "g"); }, withCard).combined).toEqual([]);
  });
});

describe("an agent's words", () => {
  it("become the person's when the person edits them", () => {
    const start = page("page-1", [{ id: "a", ref: { block: "src/routes/orders.ts#createOrder" }, at: [0, 0], description: { text: "Old.", author: "agent:claude", signature: "s" } }], {
      links: [{ id: "l", from: "a", to: "a", kind: "http", label: { text: "calls", author: "agent:claude", signature: "s" }, author: "agent:claude" }],
      names: [{ key: "k", text: "Check", author: "agent:claude", signature: "s" }],
    });
    const edited = onPage((draft) => {
      setCardDescription(draft, "a", "New.", "person:maya");
      setLinkLabel(draft, "l", "posts", "person:maya");
      setName(draft, "k", "Checked?", "person:maya");
    }, start);
    expect(edited.cards[0]?.description).toEqual({ text: "New.", author: "person:maya", signature: "" });
    expect(edited.links[0]?.label).toEqual({ text: "posts", author: "person:maya", signature: "" });
    expect(edited.names[0]).toEqual({ key: "k", text: "Checked?", author: "person:maya", signature: "" });
  });
});

describe("names", () => {
  it("are one per key, sorted as the file writes them, replaced by new text, cleared by blank text", () => {
    const named = onPage((draft) => {
      setName(draft, "key-k", "Paid?", "person:me");
      setName(draft, "key-a", "First?", "person:me");
      setName(draft, "key-k", "Valid?", "person:me");
    });
    expect(named.names.map((name) => name.key)).toEqual(["key-a", "key-k"]);
    expect(named.names.map((name) => name.text)).toEqual(["First?", "Valid?"]);
    const cleared = onPage((draft) => { setName(draft, "key-k", "", "person:me"); }, named);
    expect(cleared.names).toHaveLength(1);
  });
});

describe("pages", () => {
  it("are added after the given page with the smallest free id, titled Page N", () => {
    const start = boardDocument([page("page-1"), page("page-3")]);
    const result = onBoard((draft) => {
      expect(nextPageId(draft)).toBe("page-2");
      expect(addPage(draft, { after: "page-1" })).toBe("page-2");
      addPage(draft, { title: "Refunds" });
    }, start);
    expect(result.pages.map((item) => [item.id, item.title])).toEqual([
      ["page-1", "Page page-1"],
      ["page-2", "Page 3"],
      ["page-3", "Page page-3"],
      ["page-4", "Refunds"],
    ]);
    expect(result.pages[1]).toMatchObject({ defaultView: "compact", names: [], combined: [] });
  });

  it("move up and down, and never delete the only page", () => {
    const start = boardDocument([page("page-1"), page("page-2"), page("page-3")]);
    const result = onBoard((draft) => {
      movePage(draft, "page-3", -1);
      movePage(draft, "page-3", -1);
      movePage(draft, "page-3", -1);
    }, start);
    expect(result.pages.map((item) => item.id)).toEqual(["page-3", "page-1", "page-2"]);
    const one = onBoard((draft) => {
      deletePage(draft, "page-1");
    });
    expect(one.pages).toHaveLength(1);
  });
});

describe("views and open calls", () => {
  const CARD: Card = { id: "create-order", ref: { block: "key-create" }, at: [0, 0] };
  const CALL = "key-create-call";
  const INNER = "key-place-call";

  it("expands a card by saving its view, and opens calls in place as paths that close with what is inside them", () => {
    let next = onPage((draft) => {
      draft.cards.push({ ...CARD });
      setCardView(draft, CARD.id, "expanded");
      toggleOpen(draft, CARD.id, [CALL]);
      toggleOpen(draft, CARD.id, [CALL, INNER]);
    });
    expect(next.cards[0]?.view).toBe("expanded");
    expect(next.cards[0]?.open).toEqual([[CALL], [CALL, INNER]]);
    expect(isOpen(next.cards[0] ?? CARD, [CALL, INNER])).toBe(true);
    next = onPage((draft) => {
      toggleOpen(draft, CARD.id, [CALL]);
      setCardView(draft, CARD.id, "compact");
    }, next);
    expect(next.cards[0]?.view).toBe("compact");
    expect("open" in (next.cards[0] ?? {})).toBe(false);
  });

  it("leaves how deep a path goes to the board's checks, writing it as asked", () => {
    const next = onPage((draft) => {
      draft.cards.push({ ...CARD });
      toggleOpen(draft, CARD.id, ["a", "b", "c", "d"]);
    });
    expect(openPaths(next.cards[0] ?? CARD)).toEqual([["a", "b", "c", "d"]]);
  });
});

describe("proposed steps", () => {
  const CARD: Card = { id: "create-order", ref: { block: "key-create" }, at: [0, 0] };
  const AFTER = "key-create-load";

  it("adds a proposal after a block or at the start of a body, rewords it, and removes it; removing a card keeps it", () => {
    let next = onPage((draft) => {
      draft.cards.push({ ...CARD });
      addProposal(draft, { after: AFTER, title: "Email a confirmation", author: "agent:claude" });
      addProposal(draft, { start: "key-create", title: "Check the cart", author: "agent:claude" });
    });
    expect(next.proposals).toEqual([
      { id: "proposal", after: AFTER, title: "Email a confirmation", author: "agent:claude" },
      { id: "proposal-2", start: "key-create", title: "Check the cart", author: "agent:claude" },
    ]);
    expect(uniqueId(next, "proposal")).toBe("proposal-3");
    next = onPage((draft) => {
      updateProposal(draft, "proposal", { reason: "Only the queued path emails one." }, "person:ada");
    }, next);
    expect(next.proposals?.[0]).toMatchObject({ reason: "Only the queued path emails one.", author: "person:ada" });
    next = onPage((draft) => {
      updateProposal(draft, "proposal", { reason: null }, "person:bo");
    }, next);
    expect("reason" in (next.proposals?.[0] ?? {})).toBe(false);
    // A blank title changes nothing.
    next = onPage((draft) => {
      updateProposal(draft, "proposal", { title: "  " }, "person:cy");
    }, next);
    expect(next.proposals?.[0]).toMatchObject({ title: "Email a confirmation", author: "person:bo" });
    expect(onPage((draft) => { removeCard(draft, CARD.id); }, next).proposals).toHaveLength(2);
    expect(onPage((draft) => { removeProposal(draft, "proposal"); removeProposal(draft, "proposal-2"); }, next).proposals).toBeUndefined();
  });

  it("trims new words and leaves the rest to the board's save, which counts characters as the file does", () => {
    const start = onPage((draft) => {
      addProposal(draft, { after: AFTER, title: "Email a confirmation", author: "agent:claude" });
    });
    const long = "😀".repeat(121);
    const next = onPage((draft) => {
      updateProposal(draft, "proposal", { title: `  ${long} `, reason: " why\u0085 " }, "person:ada");
    }, start);
    expect(next.proposals?.[0]).toMatchObject({ title: long, reason: "why\u0085", author: "person:ada" });
  });
});
