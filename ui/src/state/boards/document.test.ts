import { produce } from "immer";
import { describe, expect, it } from "vitest";
import type { BoardDocument, Page } from "../../ipc/generated";
import { boardDocument, page } from "../../test/workspace";
import {
  addCard,
  addLink,
  addPage,
  deletePage,
  duplicatePage,
  labelsFor,
  movePage,
  nextPageId,
  removeCard,
  setLabel,
  setLinkLabel,
  uniqueId,
} from "./document";

function onPage(recipe: (draft: Page) => void, start: Page = page("page-1")): Page {
  return produce(start, recipe);
}

function onBoard(recipe: (draft: BoardDocument) => void, start: BoardDocument = boardDocument()): BoardDocument {
  return produce(start, recipe);
}

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
