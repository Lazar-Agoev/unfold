// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TextSearchFile } from "../../ipc/generated";
import { useTabs } from "../../state/tabs";
import { fixture, installFakeEngine, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { highlightChars, offsetsToRanges } from "./highlight";
import { SearchView, searchGroups, textCountLabel, useSearchView } from "./SearchView";
import { runTextSearch, useTextSearch } from "./textSearch";

const boards = vi.hoisted(() => ({
  openBlockCard: vi.fn<(key: string) => void>(),
  openEntrance: vi.fn<(key: string) => void>(),
}));

vi.mock("../../state/boards/api", () => boards);

const ORDERS: TextSearchFile = {
  path: "src/routes/orders.ts",
  matches: [
    { line: 32, text: "  const order = orderService.place(draft);", column: 0, ranges: [[29, 34]] },
    { line: 40, text: "  // place it later", column: 0, ranges: [[5, 10]] },
  ],
};
const SERVICE: TextSearchFile = {
  path: "src/services/order-service.ts",
  matches: [{ line: 13, text: "  place(draft: OrderDraft): Order {", column: 0, ranges: [[2, 7]] }],
};

let engine: FakeEngine;
let aborted: string[];

beforeEach(() => {
  aborted = [];
  useSearchView.setState({ query: "", tab: "all", regex: false, caseSensitive: false, wholeWord: false });
  runTextSearch(null);
  useTabs.setState({ tabs: [], activeId: null });
  boards.openBlockCard.mockClear();
  engine = installFakeEngine({
    handlers: { search: () => fixture("search", "search_place") },
    streams: {
      search_text: (args, send, signal) => {
        const query = String(args["query"]);
        if (query !== "place") {
          // A slow search that only ends when cancelled.
          send({ path: "src/stale.ts", matches: [{ line: 1, text: query, column: 0, ranges: [[0, 2]] }] });
          return new Promise((_resolve, reject) => {
            signal.addEventListener("abort", () => {
              aborted.push(query);
              // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors -- the bridge rejects with plain data.
              reject({ code: "cancelled", message: "Cancelled" });
            });
          });
        }
        send(ORDERS);
        send(SERVICE);
        return Promise.resolve({ files: 2, matches: 3, searched: 34, truncated: false, filesTruncated: false });
      },
    },
  });
});

afterEach(() => {
  runTextSearch(null);
  engine.uninstall();
});

describe("highlighting", () => {
  it("merges matched characters into ranges and marks them", () => {
    expect(offsetsToRanges([4, 0, 1, 2])).toEqual([
      [0, 3],
      [4, 5],
    ]);
    const { container } = renderWithClient(<span>{highlightChars("place", [0, 1])}</span>);
    expect(container.querySelector("mark")?.textContent).toBe("pl");
  });
});

describe("searchGroups", () => {
  it("groups names and text, and narrows to one kind by tab", () => {
    const names = fixture("search", "search_place");
    const all = searchGroups("all", names, [ORDERS, SERVICE], textCountLabel([ORDERS, SERVICE], false));
    expect(all.map((group) => [group.id, group.count])).toEqual([
      ["definitions", "1"],
      ["entrances", "0"],
      ["files", "0"],
      ["text", "3 matches in 2 files"],
    ]);
    // Text items: a header row per file, then its matching lines.
    expect(all[3]?.items.map((item) => item.kind)).toEqual(["textFile", "textMatch", "textMatch", "textFile", "textMatch"]);
    expect(searchGroups("definitions", names, [ORDERS], null).map((group) => group.id)).toEqual(["definitions"]);
    expect(textCountLabel([ORDERS], true)).toBe("2 matches+ in 1 file");
  });
});

describe("SearchView", () => {
  it("streams text results, cancelling the search a newer query replaces", async () => {
    renderWithClient(<SearchView />);
    const field = screen.getByRole("searchbox", { name: "Search" });
    fireEvent.change(field, { target: { value: "pl" } });
    await waitFor(() => {
      expect(useTextSearch.getState().signature).toContain('"pl"');
    });
    fireEvent.change(field, { target: { value: "place" } });

    expect(await screen.findByText("3 matches in 2 files")).toBeTruthy();
    expect(aborted).toEqual(["pl"]);
    expect(screen.queryByText("src/stale.ts")).toBeNull();
    expect(screen.getByText("Definitions", { selector: ".vlist-header-label" })).toBeTruthy();
    expect(screen.getByText("OrderService.")).toBeTruthy();
    expect(engine.callsOf("search_text").map((args) => args["query"])).toEqual(["pl", "place"]);
  });

  it("opens definitions as cards and text matches as source tabs at their line", async () => {
    useSearchView.setState({ query: "place" });
    renderWithClient(<SearchView />);
    fireEvent.click(await screen.findByRole("option", { name: /^OrderService\.place/ }));
    expect(boards.openBlockCard).toHaveBeenCalledWith("src/services/order-service.ts#class:OrderService/method:place");

    fireEvent.click(await screen.findByRole("option", { name: /^Line 32/ }));
    expect(useTabs.getState().tabs[0]).toMatchObject({ kind: "source", path: "src/routes/orders.ts", line: 32 });
  });

  it("leaves text out of the Definitions tab", async () => {
    useSearchView.setState({ query: "place", tab: "definitions" });
    renderWithClient(<SearchView />);
    await screen.findByText("Definitions", { selector: ".vlist-header-label" });
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(engine.callsOf("search_text")).toEqual([]);
  });
});
