// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SearchHit, SearchResults } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { CodeSearch } from "./CodeSearch";

const P = "p-shop000000";

function hit(key: string, name: string): SearchHit {
  return { project: P, key, name, qualifiedName: name, path: "shop/service.py", kind: "function", line: 9, app: null, score: 600 };
}

function results(query: string, hits: SearchHit[]): SearchResults {
  return {
    query,
    entrances: { hits: [], total: 0, truncated: false },
    definitions: { hits, total: hits.length, truncated: false },
    files: { hits: [], total: 0, truncated: false },
  };
}

let engine: FakeEngine | null = null;

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

describe("CodeSearch", () => {
  it("picks nothing on Enter while the list still answers what was typed before", async () => {
    engine = installFakeEngine({ handlers: { search: () => results("order", [hit("load-order-key", "load_order")]) } });
    const onPick = vi.fn();
    renderWithClient(<CodeSearch project={P} onPick={onPick} />);
    const input = screen.getByRole("combobox", { name: "Code" });
    fireEvent.change(input, { target: { value: "order" } });
    await screen.findByRole("option");

    let answer: (answered: SearchResults) => void = () => undefined;
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        search: () =>
          new Promise<SearchResults>((resolve) => {
            answer = resolve;
          }),
      },
    });
    fireEvent.change(input, { target: { value: "place" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 200));
    });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onPick).not.toHaveBeenCalled();

    await act(async () => {
      answer(results("place", [hit("place-order-key", "place_order")]));
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(screen.getByRole("option").textContent).toContain("place_order");
    });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onPick).toHaveBeenCalledWith(hit("place-order-key", "place_order"));
  });
});
