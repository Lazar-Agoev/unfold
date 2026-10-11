import { produce } from "immer";
import { describe, expect, it } from "vitest";
import type { BoardDocument, Card, Page } from "../../ipc/generated";
import { boardDocument, page } from "../../test/workspace";
import {
  addCard,
  addGroup,
  addLink,
  addPage,
  deletePage,
  duplicatePage,
  isFreeId,
  keyDisplayName,
  labelsFor,
  movePage,
  nextPageId,
  removeCard,
  setCardDescription,
  setDefaultView,
  setLabel,
  setLinkLabel,
  uniqueId,
  updateGroup,
} from "./document";

function onPage(recipe: (draft: Page) => void, start: Page = page("page-1")): Page {
  return produce(start, recipe);
}

function onBoard(recipe: (draft: BoardDocument) => void, start: BoardDocument = boardDocument()): BoardDocument {
  return produce(start, recipe);
}

describe("keyDisplayName", () => {
  it("names a block by its last two steps, a file by its name, and an entrance by its method and trigger", () => {
    expect(keyDisplayName("src/services/order-service.ts#class:OrderService/method:place")).toBe("OrderService.place");
    expect(keyDisplayName("src/a.ts#function:alpha/if[0]")).toBe("alpha.if");
    expect(keyDisplayName("src/a.ts#")).toBe("a.ts");
    expect(keyDisplayName('entrance:["express.route","http","POST","/orders","requireUser"]')).toBe("POST /orders");
  });

  it("reads the steps after the last `#`, as a path may hold one", () => {
    expect(keyDisplayName("src/c#x/orders.ts#function:alpha")).toBe("alpha");
    expect(keyDisplayName("src/c#/orders.ts#")).toBe("orders.ts");
  });
});

describe("card, group, and link ids", () => {
  it("are unique together within the page, slugged, and at most 100 characters", () => {
    const result = onPage((draft) => {
      addCard(draft, { ref: { block: "src/a.ts#function:createOrder" }, at: [0.4, 9.6] });
      addCard(draft, { ref: { block: "src/a.ts#function:createOrder", project: "payments" }, at: [0, 0] });
      draft.groups.push({ id: "create-order-3", card: "create-order", title: "x", members: [], collapsed: false });
      addCard(draft, { ref: { block: "src/b.ts#function:createOrder" }, at: [0, 0] });
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

describe("ids an agent names", () => {
  it("are free when well formed and no item of the page has them", () => {
    const start = page("page-1", [{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }]);
    expect(isFreeId(start, "checks")).toBe(true);
    expect(isFreeId(start, "a")).toBe(false);
    expect(isFreeId(start, "-checks")).toBe(false);
    expect(isFreeId(start, "x".repeat(101))).toBe(false);
  });
});

describe("step groups", () => {
  const card: Card = { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] };

  it("take a named id, start collapsed when asked, and record who titled them", () => {
    const result = onPage((draft) => {
      expect(addGroup(draft, "a", ["k1"], "Checks", { id: "guard", collapsed: true, author: "agent:claude" })).toBe("guard");
      expect(addGroup(draft, "a", ["k2"], "Rest")).toBe("rest");
    }, page("page-1", [card]));
    expect(result.groups).toEqual([
      { id: "guard", card: "a", title: "Checks", members: ["k1"], collapsed: true, author: "agent:claude" },
      { id: "rest", card: "a", title: "Rest", members: ["k2"], collapsed: false },
    ]);
  });

  it("change their statements, and a new title is its writer's: no one else's", () => {
    const start = page("page-1", [card], { groups: [{ id: "g", card: "a", title: "Checks", members: ["k1"], collapsed: false, author: "agent:claude" }] });
    const same = onPage((draft) => { updateGroup(draft, "g", { title: "Checks", author: "person:maya", members: ["k1", "k2"] }); }, start);
    expect(same.groups[0]).toEqual({ id: "g", card: "a", title: "Checks", members: ["k1", "k2"], collapsed: false, author: "agent:claude" });
    expect(onPage((draft) => { updateGroup(draft, "g", { title: "Guard", author: "person:maya" }); }, start).groups[0]?.author).toBe("person:maya");
    expect(onPage((draft) => { updateGroup(draft, "g", { title: "Guard" }); }, start).groups[0]).not.toHaveProperty("author");
  });
});

describe("an agent's words", () => {
  it("become the person's when the person edits them", () => {
    const start = page("page-1", [{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0], description: { text: "Old.", author: "agent:claude", signature: "s" } }], {
      links: [{ id: "l", from: "a", to: "a", label: { text: "calls", author: "agent:claude", signature: "s" } }],
      labels: [{ key: "k", text: "Check", author: "agent:claude", signature: "s" }],
    });
    const edited = onPage((draft) => {
      setCardDescription(draft, "a", "New.", "person:maya");
      setLinkLabel(draft, "l", "posts", "person:maya");
      setLabel(draft, undefined, "k", "Checked?", "person:maya");
    }, start);
    expect(edited.cards[0]?.description).toEqual({ text: "New.", author: "person:maya", signature: "" });
    expect(edited.links[0]?.label).toEqual({ text: "posts", author: "person:maya", signature: "" });
    expect(edited.labels[0]).toEqual({ key: "k", text: "Checked?", author: "person:maya", signature: "" });
  });
});

describe("the page's default view", () => {
  it("is set for the cards without their own", () => {
    expect(onPage((draft) => { setDefaultView(draft, "diagram"); }).defaultView).toBe("diagram");
  });
});

describe("links", () => {
  it("join two different cards of the page, take a label, and go with either card", () => {
    const start = page("page-1", [
      { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] },
      { id: "b", ref: { block: "src/b.ts#function:b" }, at: [0, 0] },
    ]);
    const linked = onPage((draft) => {
      expect(addLink(draft, "a", "a")).toBeUndefined();
      expect(addLink(draft, "a", "missing")).toBeUndefined();
      const id = addLink(draft, "a", "b") ?? "";
      setLinkLabel(draft, id, "calls over HTTP", "person:me");
    }, start);
    expect(linked.links).toEqual([{ id: "a-to-b", from: "a", to: "b", label: { text: "calls over HTTP", author: "person:me", signature: "" } }]);
    expect(onPage((draft) => { setLinkLabel(draft, "a-to-b", " ", "person:me"); }, linked).links[0]).toEqual({ id: "a-to-b", from: "a", to: "b" });
    expect(onPage((draft) => { removeCard(draft, "b"); }, linked).links).toEqual([]);
  });
});

describe("labels", () => {
  it("are per project and key, sorted as the file writes them, cleared by blank text", () => {
    const labeled = onPage((draft) => {
      setLabel(draft, "payments", "src/k.ts#function:k/if[0]", "Paid?", "person:me");
      setLabel(draft, undefined, "src/k.ts#function:k/if[0]", "Valid?", "person:me");
      setLabel(draft, undefined, "src/a.ts#function:a/if[0]", "First?", "person:me");
    });
    expect(labeled.labels.map((label) => [label.project ?? "", label.key])).toEqual([
      ["", "src/a.ts#function:a/if[0]"],
      ["", "src/k.ts#function:k/if[0]"],
      ["payments", "src/k.ts#function:k/if[0]"],
    ]);
    expect(labelsFor(labeled.labels, "payments")).toEqual({ "src/k.ts#function:k/if[0]": "Paid?" });
    expect(labelsFor(labeled.labels, undefined)["src/k.ts#function:k/if[0]"]).toBe("Valid?");
    const cleared = onPage((draft) => { setLabel(draft, "payments", "src/k.ts#function:k/if[0]", "", "person:me"); }, labeled);
    expect(cleared.labels).toHaveLength(2);
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
  });

  it("duplicate with their content, move up and down, and never delete the only page", () => {
    const start = boardDocument([page("page-1", [{ id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] }]), page("page-2")]);
    const result = onBoard((draft) => {
      expect(duplicatePage(draft, "page-1")).toBe("page-3");
      movePage(draft, "page-2", -1);
      movePage(draft, "page-2", -1);
    }, start);
    expect(result.pages.map((item) => item.id)).toEqual(["page-2", "page-1", "page-3"]);
    expect(result.pages[2]).toMatchObject({ title: "Page page-1 copy", cards: [{ id: "a" }] });
    const one = onBoard((draft) => {
      deletePage(draft, "page-1");
    });
    expect(one.pages).toHaveLength(1);
  });
});
