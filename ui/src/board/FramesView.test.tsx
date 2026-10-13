// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, CardView, Frame, Page } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardSidebar } from "./BoardSidebar";
import { BoardView } from "./BoardView";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const CREATE = "src/routes/orders.ts#function:createOrder";
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const SLACK = "src/lib/slack.ts#variable:slack/method:post";

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

const cards: Card[] = [
  { id: "create-order", ref: { block: CREATE }, at: [0, 0] },
  { id: "place", ref: { block: PLACE }, at: [700, 0] },
  { id: "slack", ref: { block: SLACK }, at: [700, 400] },
];
const frame = (id: string, members: string[], extra: Partial<Frame> = {}): Frame => ({ id, title: id, members, look: "box", author: "agent:claude", ...extra });

let engine: FakeEngine;

/** createOrder's golden card (it calls OrderService.place and slack.post); the others are copies of it, renamed. */
function model(key: string): CardView {
  const card = fixture("card", "card_create_order");
  if (key === CREATE || card.type !== "block") {
    return card;
  }
  const name = key === PLACE ? "OrderService.place" : "slack.post";
  return { ...card, header: { ...card.header, key, name, qualifiedName: name } };
}

function open(overrides: Partial<Page>, defaultView: "code" | "diagram" = "diagram") {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, { defaultView, ...overrides })])));
}

function current(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("the page isn't open");
  }
  return found;
}

function withClient(ui: React.ReactNode) {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  queryClient.setQueryData(engineKey("app_info", {}), { username: "lazar", engineVersion: "0" });
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);
}

function renderBoard() {
  return withClient(
    <div style={{ width: 1200, height: 800 }}>
      <BoardView board={REF} />
    </div>,
  );
}

const frameElement = (container: HTMLElement, id: string) => container.querySelector<HTMLElement>(`[data-frame-id="${id}"]`);
/** An element by its accessible name (the canvas's nodes stay hidden from role queries until React Flow measures them, which jsdom can't). */
const named = (container: HTMLElement, label: string) => present(container.querySelector<HTMLElement>(`[aria-label="${label}"]`));

/** `value`, once it is there (inside `waitFor`, which tries again until it is). */
function present<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error("not drawn yet");
  }
  return value;
}

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => model(key),
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: () => boardState(P, ID),
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("frames on the canvas", () => {
  it("draw a box and a zone behind what they hold, nested, with their icon, title, description, and the agent mark", async () => {
    open({
      frames: [
        frame("system", ["server", "slack"], { look: "zone", title: "Order system", description: { text: "Everything that takes orders.", author: "person:maya", signature: "" } }),
        frame("server", ["create-order", "place"], { icon: "api", title: "Order API" }),
      ],
    });
    const { container } = renderBoard();
    const zone = await waitFor(() => present(frameElement(container, "system")));
    const box = present(frameElement(container, "server"));
    expect(zone.classList.contains("look-zone")).toBe(true);
    expect(box.classList.contains("look-box")).toBe(true);
    expect(zone.querySelector(".frame-title-text")?.textContent).toBe("Order system");
    expect(zone.querySelector(".frame-description")?.textContent).toBe("Everything that takes orders.");
    expect(box.querySelector(".frame-icon")?.getAttribute("data-icon")).toBe("frame-api");
    expect(zone.querySelector(".frame-icon")?.getAttribute("data-icon")).toBe("frame");
    expect(within(box).getByText("agent")).not.toBeNull();
    // Behind the lines and cards, the zone behind the box it holds; the zone is larger than the box.
    const layer = (element: HTMLElement) => Number(element.closest<HTMLElement>(".react-flow__node")?.style.zIndex);
    expect(layer(zone)).toBeLessThan(layer(box));
    expect(layer(box)).toBeLessThan(0);
    expect(Number.parseFloat(zone.style.width)).toBeGreaterThan(Number.parseFloat(box.style.width));
    expect(container.querySelector('.board-card[data-card-id="create-order"]')).not.toBeNull();
  });

  it("draw a collapsed frame as one node that says what it holds, its members' arrows leaving from it", async () => {
    open({ frames: [frame("server", ["create-order", "slack"], { collapsed: true, description: { text: "Takes orders.", author: "agent:claude", signature: "" } })] });
    const { container } = renderBoard();
    const node = await waitFor(() => {
      const found = frameElement(container, "server");
      expect(found?.querySelector('.card-port[data-handleid="node"]')).not.toBeNull();
      return present(found);
    });
    expect(node.classList.contains("frame-node")).toBe(true);
    expect(node.querySelector(".frame-node-description")?.textContent).toBe("Takes orders.");
    expect(node.querySelector(".frame-node-holds")?.textContent).toBe("2 functions");
    expect(container.querySelector('.board-card[data-card-id="create-order"]')).toBeNull();
    expect(container.querySelector('.board-card[data-card-id="place"]')).not.toBeNull();
  });

  it("collapse and expand with their chevron, one undo step each", async () => {
    open({ frames: [frame("server", ["create-order", "place"])] });
    const { container } = renderBoard();
    await waitFor(() => present(frameElement(container, "server")));
    act(() => {
      named(container, "Collapse the frame server").click();
    });
    expect(current().frames?.[0]?.collapsed).toBe(true);
    await waitFor(() => {
      expect(frameElement(container, "server")?.classList.contains("frame-node")).toBe(true);
    });
    await waitFor(() => named(container, "Expand the frame server"));
    act(() => {
      named(container, "Expand the frame server").click();
    });
    expect(current().frames?.[0]).not.toHaveProperty("collapsed");
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(2);
  });

  it("have a menu: rename in place, collapse, and remove the frame keeping its cards", async () => {
    open({ frames: [frame("server", ["create-order", "place"])] });
    const { container } = renderBoard();
    const box = await waitFor(() => present(frameElement(container, "server")));
    fireEvent.contextMenu(present(box.querySelector(".frame-title")));
    const menu = await screen.findByRole("menu", { name: "Frame actions" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Rename…", "Describe…", "Collapse", "Copy link", "Remove frame (keep cards)"]);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "frame", frameId: "server" });
    act(() => {
      within(menu).getByRole("menuitem", { name: "Rename…" }).click();
    });
    const field = await waitFor(() => named(container, "Frame title"));
    fireEvent.change(field, { target: { value: "Order API" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(current().frames?.[0]).toMatchObject({ title: "Order API", author: "person:lazar" });

    fireEvent.contextMenu(present(frameElement(container, "server")?.querySelector(".frame-title")));
    act(() => {
      within(screen.getByRole("menu", { name: "Frame actions" })).getByRole("menuitem", { name: "Remove frame (keep cards)" }).click();
    });
    expect(current()).not.toHaveProperty("frames");
    expect(current().cards).toHaveLength(3);
  });
});

describe("frames in the sidebar", () => {
  it("merges drawn links at a collapsed frame into one line whose label counts them and selects nothing", async () => {
    open({
      frames: [frame("server", ["create-order", "place"], { collapsed: true })],
      links: [
        { id: "first", from: "create-order", to: "slack", label: { text: "posts", author: "person:maya", signature: "" } },
        { id: "second", from: "place", to: "slack" },
      ],
    });
    const { container } = renderBoard();
    const label = await waitFor(() => present(container.querySelector<HTMLElement>(".board-link-label")));
    expect(label.textContent).toBe("2 links");
    act(() => {
      fireEvent.click(label);
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "none" });
  });

  it("Selected shows a frame's title, look, author, description, members, and lines in and out", () => {
    open({
      frames: [frame("system", ["server"], { look: "zone", title: "Order system" }), frame("server", ["create-order", "place"], { title: "Order API" })],
      links: [{ id: "to-slack", from: "place", to: "slack", label: { text: "notifies", author: "person:maya", signature: "" } }],
    });
    withClient(<BoardSidebar board={REF} />);
    act(() => {
      store.select(PAGE, { kind: "frame", frameId: "server" });
    });
    const selected = screen.getByRole("region", { name: "Selected" });
    expect(within(selected).getByRole<HTMLInputElement>("textbox", { name: "Frame title" }).value).toBe("Order API");
    expect(within(selected).getByText(/Box frame/).textContent).toContain("Box frame in Order system · drawn by Claude · agent");
    fireEvent.change(within(selected).getByRole("textbox", { name: "Frame description" }), { target: { value: "Takes orders." } });
    expect(current().frames?.[1]?.description).toEqual({ text: "Takes orders.", author: "person:lazar", signature: "" });
    const values = [...selected.querySelectorAll(".relation-row")].map((row) => row.textContent);
    expect(values).toEqual(["HoldscreateOrder", "OrderService.place", "Lines→ slack.post · notifies"]);
    act(() => {
      within(selected).getByRole("button", { name: "Collapse" }).click();
    });
    expect(current().frames?.[1]?.collapsed).toBe(true);
    act(() => {
      within(selected).getByRole("button", { name: "Remove frame" }).click();
    });
    expect(current().frames).toEqual([frame("system", ["create-order", "place"], { look: "zone", title: "Order system" })]);
  });
});
