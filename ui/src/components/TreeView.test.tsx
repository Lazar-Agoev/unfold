// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useCallback, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { TreeView, type TreeChildren } from "./TreeView";

interface Node {
  readonly id: string;
  readonly name: string;
  readonly children?: readonly Node[];
}

const ROOTS: readonly Node[] = [
  {
    id: "src",
    name: "src",
    children: [
      { id: "src/db", name: "db", children: [{ id: "src/db/pool.ts", name: "pool.ts" }] },
      { id: "src/routes", name: "routes", children: [{ id: "src/routes/orders.ts", name: "orders.ts" }] },
      { id: "src/server.ts", name: "server.ts" },
    ],
  },
  { id: "package.json", name: "package.json" },
];

function StaticTree(props: { onSelect?: (node: Node) => void; onOpen?: (node: Node) => void; onContextMenu?: (node: Node) => void }) {
  const getChildren = useCallback((node: Node) => node.children, []);
  return (
    <TreeView<Node>
      label="Files"
      roots={ROOTS}
      getId={(node) => node.id}
      getText={(node) => node.name}
      isExpandable={(node) => node.children !== undefined}
      getChildren={getChildren}
      renderRow={(node) => ({ label: node.name })}
      {...props}
    />
  );
}

const tree = () => screen.getByRole("tree", { name: "Files" });
const activeRow = () => {
  const id = tree().getAttribute("aria-activedescendant");
  return id === null ? null : document.getElementById(id);
};
const press = (key: string, init: KeyboardEventInit = {}) => {
  fireEvent.keyDown(tree(), { key, ...init });
};

describe("TreeView keyboard navigation", () => {
  it("moves, expands, enters, and leaves branches like a WAI-ARIA tree", () => {
    render(<StaticTree />);
    tree().focus();
    expect(activeRow()?.textContent).toBe("src");
    expect(activeRow()?.getAttribute("aria-expanded")).toBe("false");

    press("ArrowRight");
    expect(activeRow()?.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getAllByRole("treeitem").map((row) => row.textContent)).toEqual([
      "src",
      "db",
      "routes",
      "server.ts",
      "package.json",
    ]);

    press("ArrowRight");
    expect(activeRow()?.textContent).toBe("db");
    expect(activeRow()?.getAttribute("aria-level")).toBe("2");
    expect(activeRow()?.getAttribute("aria-posinset")).toBe("1");
    expect(activeRow()?.getAttribute("aria-setsize")).toBe("3");

    press("ArrowDown");
    press("ArrowDown");
    expect(activeRow()?.textContent).toBe("server.ts");
    press("ArrowRight");
    expect(activeRow()?.textContent).toBe("server.ts");

    press("ArrowLeft");
    expect(activeRow()?.textContent).toBe("src");
    press("ArrowLeft");
    expect(activeRow()?.getAttribute("aria-expanded")).toBe("false");
    expect(screen.getAllByRole("treeitem")).toHaveLength(2);

    press("End");
    expect(activeRow()?.textContent).toBe("package.json");
    press("Home");
    expect(activeRow()?.textContent).toBe("src");
  });

  it("expands all siblings with * and jumps by typed name", () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    render(<StaticTree />);
    tree().focus();
    press("ArrowRight");
    press("ArrowRight");
    press("*");
    expect(screen.getAllByRole("treeitem").map((row) => row.textContent)).toContain("orders.ts");
    press("s");
    expect(activeRow()?.textContent).toBe("server.ts");
    // After a pause, typing starts a new prefix; letters typed in quick succession extend it.
    vi.setSystemTime(Date.now() + 1000);
    press("p");
    press("a");
    expect(activeRow()?.textContent).toBe("package.json");
    vi.useRealTimers();
  });

  it("selects with Space and clicks, opens with Enter and double-click, and opens the context menu from the keyboard", () => {
    const onSelect = vi.fn();
    const onOpen = vi.fn();
    const onContextMenu = vi.fn();
    render(<StaticTree onSelect={onSelect} onOpen={onOpen} onContextMenu={onContextMenu} />);
    tree().focus();
    press("End");
    press(" ");
    expect(onSelect).toHaveBeenLastCalledWith(expect.objectContaining({ id: "package.json" }));
    press("Enter");
    expect(onOpen).toHaveBeenLastCalledWith(expect.objectContaining({ id: "package.json" }));
    press("F10", { shiftKey: true });
    expect(onContextMenu).toHaveBeenLastCalledWith(expect.objectContaining({ id: "package.json" }), expect.anything());

    fireEvent.click(screen.getByText("src"));
    expect(onSelect).toHaveBeenLastCalledWith(expect.objectContaining({ id: "src" }));
    expect(screen.getByText("src").closest("[role=treeitem]")?.getAttribute("aria-expanded")).toBe("true");
    fireEvent.doubleClick(screen.getByText("server.ts"));
    expect(onOpen).toHaveBeenLastCalledWith(expect.objectContaining({ id: "src/server.ts" }));
  });

  it("leaves `/` and `]` to the window's shortcuts", () => {
    render(<StaticTree />);
    tree().focus();
    const slash = fireEvent.keyDown(tree(), { key: "/" });
    expect(slash).toBe(true);
  });
});

describe("TreeView lazy children", () => {
  function LazyTree({ onLoad }: { onLoad: (id: string) => void }) {
    const [loaded, setLoaded] = useState<Readonly<Record<string, TreeChildren<Node>>>>({});
    const getChildren = useCallback((node: Node) => loaded[node.id], [loaded]);
    return (
      <>
        <button
          type="button"
          onClick={() => {
            setLoaded({ src: [{ id: "src/a.ts", name: "a.ts" }] });
          }}
        >
          deliver
        </button>
        <button
          type="button"
          onClick={() => {
            setLoaded({ src: { status: "error", message: "Couldn't read src" } });
          }}
        >
          fail
        </button>
        <TreeView<Node>
          label="Files"
          roots={[{ id: "src", name: "src" }]}
          getId={(node) => node.id}
          getText={(node) => node.name}
          isExpandable={() => true}
          getChildren={getChildren}
          onLoadChildren={(node) => {
            onLoad(node.id);
          }}
          renderRow={(node) => ({ label: node.name })}
        />
      </>
    );
  }

  it("shows a loading row, asks once, and renders children when they arrive", () => {
    const onLoad = vi.fn();
    render(<LazyTree onLoad={onLoad} />);
    tree().focus();
    press("ArrowRight");
    expect(screen.getByText("Loading…")).toBeTruthy();
    expect(onLoad).toHaveBeenCalledTimes(1);
    expect(screen.getByText("src").closest("[role=treeitem]")?.getAttribute("aria-busy")).toBe("true");

    act(() => {
      screen.getByText("deliver").click();
    });
    expect(screen.queryByText("Loading…")).toBeNull();
    press("ArrowDown");
    expect(activeRow()?.textContent).toBe("a.ts");
    expect(onLoad).toHaveBeenCalledTimes(1);
  });

  it("shows a load error in place of the children", () => {
    render(<LazyTree onLoad={vi.fn()} />);
    tree().focus();
    press("ArrowRight");
    act(() => {
      screen.getByText("fail").click();
    });
    expect(screen.getByText("Couldn't read src")).toBeTruthy();
    // Status rows are skipped by navigation.
    press("ArrowDown");
    expect(activeRow()?.textContent).toBe("src");
  });
});

describe("TreeView virtualization", () => {
  it("renders only the rows in view", () => {
    const many: Node[] = Array.from({ length: 5000 }, (_, index) => ({ id: `f${index}`, name: `file${index}.ts` }));
    const getChildren = () => undefined;
    render(
      <TreeView<Node>
        label="Files"
        roots={many}
        getId={(node) => node.id}
        getText={(node) => node.name}
        isExpandable={() => false}
        getChildren={getChildren}
        renderRow={(node) => ({ label: node.name })}
      />,
    );
    const rendered = screen.getAllByRole("treeitem").length;
    expect(rendered).toBeGreaterThan(5);
    expect(rendered).toBeLessThan(60);
    tree().focus();
    press("End");
    // jsdom can't scroll, so the row itself stays unrendered; the tree points at it.
    expect(tree().getAttribute("aria-activedescendant")).toMatch(/-4999$/);
  });
});
