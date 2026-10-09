// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { VirtualList, type ListGroup } from "./VirtualList";

interface Result {
  readonly id: string;
  readonly name: string;
}

const groups = (definitions: number, entrances: number): ListGroup<Result>[] => [
  {
    id: "definitions",
    label: "Definitions",
    count: definitions,
    items: Array.from({ length: definitions }, (_, index) => ({ id: `d${index}`, name: `place${index}` })),
  },
  { id: "files", label: "Files", count: 0, items: [] },
  {
    id: "entrances",
    label: "Entrances",
    count: entrances,
    items: Array.from({ length: entrances }, (_, index) => ({ id: `e${index}`, name: `POST /orders/${index}` })),
  },
];

function Results({ data, onActivate }: { data: ListGroup<Result>[]; onActivate?: (item: Result) => void }) {
  return (
    <VirtualList<Result>
      label="Search results"
      groups={data}
      getId={(item) => item.id}
      renderItem={(item) => <span>{item.name}</span>}
      {...(onActivate ? { onActivate } : {})}
      empty={<p>No results</p>}
    />
  );
}

const list = () => screen.getByRole("listbox", { name: "Search results" });
const active = () => {
  const id = list().getAttribute("aria-activedescendant");
  return id === null ? null : document.getElementById(id);
};

describe("VirtualList", () => {
  it("renders group headers and their items, skipping empty groups", () => {
    render(<Results data={groups(2, 2)} />);
    expect(screen.getByText("Definitions")).toBeTruthy();
    expect(screen.getByText("Entrances")).toBeTruthy();
    expect(screen.queryByText("Files")).toBeNull();
    expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual([
      "place0",
      "place1",
      "POST /orders/0",
      "POST /orders/1",
    ]);
  });

  it("renders only a window of a long list, and keeps the current group's header", () => {
    render(<Results data={groups(3000, 10)} />);
    const options = screen.getAllByRole("option");
    expect(options.length).toBeGreaterThan(5);
    expect(options.length).toBeLessThan(60);
    expect(document.querySelector(".vlist-header-sticky")?.textContent).toContain("Definitions");
  });

  it("moves between items across groups with the keyboard and activates with Enter", () => {
    const onActivate = vi.fn();
    render(<Results data={groups(2, 1)} onActivate={onActivate} />);
    fireEvent.focus(list());
    expect(active()?.textContent).toBe("place0");
    fireEvent.keyDown(list(), { key: "ArrowDown" });
    fireEvent.keyDown(list(), { key: "ArrowDown" });
    expect(active()?.textContent).toBe("POST /orders/0");
    fireEvent.keyDown(list(), { key: "ArrowDown" });
    expect(active()?.textContent).toBe("POST /orders/0");
    fireEvent.keyDown(list(), { key: "Home" });
    expect(active()?.textContent).toBe("place0");
    fireEvent.keyDown(list(), { key: "Enter" });
    expect(onActivate).toHaveBeenCalledWith({ id: "d0", name: "place0" });
    fireEvent.click(screen.getByText("POST /orders/0"));
    expect(onActivate).toHaveBeenLastCalledWith({ id: "e0", name: "POST /orders/0" });
  });

  it("shows the empty state when nothing matches", () => {
    render(<Results data={groups(0, 0)} />);
    expect(screen.getByText("No results")).toBeTruthy();
    expect(screen.queryByRole("listbox")).toBeNull();
  });
});
