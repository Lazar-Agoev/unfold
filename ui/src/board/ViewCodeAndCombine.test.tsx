// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { DialogHost } from "../dialogs/DialogHost";
import { queryClient } from "../ipc/client";
import type { Card, Group } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { closeDialog, closePeek, getPeekTarget, openPeek } from "../state/overlays";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardSidebar } from "./BoardSidebar";
import { PeekSheet } from "../peek/PeekSheet";
import { BoardView } from "./BoardView";

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

const P = "p-orders0000";
const REF = { project: P, id: ".unfold/boards/b.json" };
const PAGE = { board: REF, page: "page-1" };
const ORDERS = "src/routes/orders.ts";
const CREATE = `${ORDERS}#function:createOrder`;
const IF = `${CREATE}/if[0]`;
const DRAFT = `${CREATE}/variable:draft`;
const JOB = `${IF}/variable:job`;
const ORDER = `${CREATE}/variable:order`;
const SLACK = `${CREATE}/expression_statement[0]`;
const REPLY = `${CREATE}/expression_statement[1]`;
const ENTRANCE = "entrance:configured:import";
const IMPORT = "src/jobs/import-orders.ts#function:importOrders";

const createOrder = fixture("card", "card_create_order");
const createCard: Card = { id: "create-order", ref: { block: CREATE }, at: [0, 0], view: "flow" };
const queueGroup: Group = { id: "queue-it", card: "create-order", title: "Queue it", members: [JOB, `${IF}/expression_statement[0]`, `${IF}/return[0]`], collapsed: false };
const placeGroup: Group = { id: "place-it", card: "create-order", title: "Place it now", members: [ORDER, SLACK, REPLY], collapsed: true };

/** The node of the card's own Flow that shows `key`. */
const NODE_IDS: Readonly<Record<string, number>> = { [DRAFT]: 1, [IF]: 2, [JOB]: 3, [ORDER]: 6, [SLACK]: 7, [REPLY]: 8 };

/** createOrder's model after a compile that moved it `lines` down: every recorded line shifts. */
function movedDown(view: unknown, lines: number): unknown {
  if (Array.isArray(view)) {
    return view.map((item) => movedDown(item, lines));
  }
  if (typeof view === "object" && view !== null) {
    return Object.fromEntries(Object.entries(view).map(([name, value]) => [name, /^(start|end)Line$/.test(name) && typeof value === "number" ? value + lines : movedDown(value, lines)]));
  }
  return view;
}

/** While set, `card` answers createOrder only when the test says (a compile's refresh in flight). */
let pendingCard: Promise<unknown> | null = null;

let engine: FakeEngine;

beforeEach(() => {
  pendingCard = null;
  store.resetBoards();
  queryClient.clear();
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        if (key === CREATE) {
          return pendingCard ? (pendingCard as Promise<typeof createOrder>) : createOrder;
        }
        if (key === ENTRANCE) {
          return fixture("card", "card_first_entrance");
        }
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "not_found", message: "no such key" };
      },
      relations: () => fixture("relations", "relations_create_order"),
      file_view: () => fixture("file_view", "file_view_orders_page"),
      line_labels: () => [],
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: () => boardState(P, REF.id, store.getOpenBoard(REF)?.document),
    },
  });
});

afterEach(() => {
  act(() => {
    closePeek();
    closeDialog();
  });
  store.resetBoards();
  engine.uninstall();
});

function open(cards: readonly Card[] = [createCard], groups: readonly Group[] = []) {
  store.adoptBoardState(boardState(P, REF.id, boardDocument([page("page-1", cards, { defaultView: "flow", groups: [...groups] })])));
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  queryClient.setQueryData(engineKey("app_info", {}), { username: "maya", engineVersion: "0" });
  return render(
    <QueryClientProvider client={queryClient}>
      <div style={{ width: 1200, height: 800 }}>
        <BoardSidebar board={REF} />
        <BoardView board={REF} />
        <DialogHost />
      </div>
    </QueryClientProvider>,
  );
}

/** The element `selector` finds in `container`, once it is drawn. */
async function drawn(container: HTMLElement, selector: string): Promise<HTMLElement> {
  return waitFor(() => {
    const found = container.querySelector<HTMLElement>(selector);
    if (!found) {
      throw new Error(`${selector} isn't drawn yet`);
    }
    return found;
  });
}

/** A step of createOrder's own Flow, once drawn. */
async function step(container: HTMLElement, key: string): Promise<HTMLElement> {
  return drawn(container, `.block-card [data-flow-node="${String(NODE_IDS[key])}"]`);
}

/** The sheet's target as View code opened it, once it says how to find that source again. */
function peeked() {
  const target = getPeekTarget();
  if (!target) {
    return null;
  }
  const { resolve, ...shown } = target;
  expect(resolve).toBeTypeOf("function");
  return shown;
}

const selection = () => store.getPageView(PAGE).selection;
const selectedSection = () => screen.getByRole("region", { name: "Selected" });
const groupsNow = () => store.getOpenBoard(REF)?.document.pages[0]?.groups ?? [];

describe("Combine", () => {
  it("Shift-click selects a run of the card's own steps, drawn selected and named in Selected; a plain click resets it", async () => {
    const { container } = open();
    fireEvent.click(await step(container, ORDER));
    fireEvent.click(await step(container, REPLY), { shiftKey: true });
    expect(selection()).toEqual({ kind: "card", cardId: "create-order", key: ORDER, last: REPLY });
    await waitFor(async () => {
      expect((await step(container, SLACK)).classList.contains("selected")).toBe(true);
    });
    expect((await step(container, IF)).classList.contains("selected")).toBe(false);
    const selected = within(selectedSection());
    expect(selected.getByText("3 steps")).toBeTruthy();
    expect(selected.getByRole("button", { name: "lines 32–34" })).toBeTruthy();
    expect(selected.getByText("consecutive · one body")).toBeTruthy();

    fireEvent.click(await step(container, SLACK));
    expect(selection()).toEqual({ kind: "card", cardId: "create-order", key: SLACK });
  });

  it("Combine… in a run's menu names it in the group dialog and makes one collapsed block, authored by the person", async () => {
    const { container } = open();
    fireEvent.click(await step(container, ORDER));
    fireEvent.click(await step(container, REPLY), { shiftKey: true });
    fireEvent.contextMenu(await step(container, SLACK));
    // The menu acts on the run (the step right-clicked is in it).
    expect(selection()).toEqual({ kind: "card", cardId: "create-order", key: ORDER, last: REPLY });
    const menu = await screen.findByRole("menu");
    expect(within(menu).getByText("3 steps · createOrder · lines 32–34")).toBeTruthy();
    expect(within(menu).getByRole("menuitem", { name: /View code/ }).getAttribute("aria-disabled")).not.toBe("true");
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Combine…" }));

    const dialog = await screen.findByRole("dialog", { name: "Combine 3 steps" });
    expect(within(dialog).getByRole("textbox", { name: "Steps" })).toHaveProperty("value", "routes/orders.ts:32-34 · 3 steps");
    expect(within(dialog).getByRole("textbox", { name: "Steps" })).toHaveProperty("readOnly", true);
    // A title is required.
    fireEvent.click(within(dialog).getByRole("button", { name: "Combine" }));
    expect(within(dialog).getByText("Give the combined block a title.")).toBeTruthy();
    expect(groupsNow()).toEqual([]);
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Title" }), { target: { value: "Place it now" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Combine" }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(groupsNow()).toEqual([{ id: "place-it-now", card: "create-order", title: "Place it now", members: [ORDER, SLACK, REPLY], collapsed: true, author: "person:maya" }]);
    expect(selection()).toEqual({ kind: "group", groupId: "place-it-now" });
    const block = await drawn(container, '[data-flow-node="group:place-it-now"]');
    expect(block.querySelector(".flow-type")?.textContent).toMatch(/^COMBINED3 steps/);
  });

  it("Combine… is disabled, saying why, for one step or a run the outline rule refuses", async () => {
    const { container } = open([createCard], [queueGroup]);
    fireEvent.contextMenu(await step(container, ORDER));
    expect(selection()).toEqual({ kind: "card", cardId: "create-order", key: ORDER });
    let combine = within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Combine…" });
    expect(combine.getAttribute("aria-disabled")).toBe("true");
    expect(combine.getAttribute("title")).toBe("Shift-click the last step of the run to combine it.");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    fireEvent.click(await step(container, DRAFT));
    fireEvent.click(await step(container, IF), { shiftKey: true });
    fireEvent.contextMenu(await step(container, DRAFT));
    combine = within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Combine…" });
    expect(combine.getAttribute("aria-disabled")).toBe("true");
    expect(combine.getAttribute("title")).toBe('These lines overlap the group "Queue it". Groups never overlap or nest.');
    expect(within(selectedSection()).getByText('These lines overlap the group "Queue it". Groups never overlap or nest.')).toBeTruthy();
  });

  it("Selected for a combined block says COMBINED · in its function, opens its lines and View code, and Ungroup restores the steps", async () => {
    open([createCard], [placeGroup]);
    act(() => {
      store.select(PAGE, { kind: "group", groupId: "place-it" });
    });
    const selected = within(selectedSection());
    expect(selectedSection().querySelector(".selected-location")?.textContent).toBe("COMBINED · in createOrder");
    fireEvent.click(await selected.findByRole("button", { name: "routes/orders.ts:32–34" }));
    expect(peeked()).toEqual({ project: P, path: ORDERS, line: 32, focus: { startLine: 32, endLine: 34 }, follow: PAGE });
    act(() => {
      closePeek();
    });
    fireEvent.click(selected.getByRole("button", { name: "View code" }));
    expect(getPeekTarget()?.focus).toEqual({ startLine: 32, endLine: 34 });
    expect(selection()).toEqual({ kind: "group", groupId: "place-it" });

    fireEvent.click(selected.getByRole("button", { name: "Ungroup" }));
    expect(groupsNow()).toEqual([]);
  });
});

describe("View code", () => {
  it("a step's </> selects it and opens its source; the sheet then follows the selection, and stays for an item without source", async () => {
    const { container } = open([createCard], [placeGroup]);
    fireEvent.click(await within(await step(container, IF)).findByRole("button", { name: "View code" }));
    expect(selection()).toEqual({ kind: "card", cardId: "create-order", key: IF });
    expect(getPeekTarget()?.line).toBe(27);
    const before = store.getOpenBoard(REF)?.document;
    const camera = store.getPageView(PAGE).camera;

    fireEvent.click(await step(container, DRAFT));
    await waitFor(() => {
      expect(getPeekTarget()?.focus).toEqual({ startLine: 26, startColumn: 8, endLine: 26, endColumn: 36 });
    });
    expect(selection()).toEqual({ kind: "card", cardId: "create-order", key: DRAFT });

    // A combined block: the sheet shows its span, and the selection stays on the group.
    fireEvent.click(await drawn(container, '[data-flow-node="group:place-it"]'));
    await waitFor(() => {
      expect(getPeekTarget()?.focus).toEqual({ startLine: 32, endLine: 34 });
    });
    expect(selection()).toEqual({ kind: "group", groupId: "place-it" });

    act(() => {
      store.select(PAGE, { kind: "none" });
    });
    expect(getPeekTarget()?.focus).toEqual({ startLine: 32, endLine: 34 });
    // Following never edits the page or moves the camera.
    expect(store.getOpenBoard(REF)?.document).toBe(before);
    expect(store.getPageView(PAGE).camera).toBe(camera);
  });

  it("when a recompile moves the selected step, its highlight moves with it, and isn't drawn while the card refreshes", async () => {
    const { container } = open();
    render(
      <QueryClientProvider client={queryClient}>
        <PeekSheet />
      </QueryClientProvider>,
    );
    fireEvent.click(await within(await step(container, IF)).findByRole("button", { name: "View code" }));
    expect(getPeekTarget()?.focus).toEqual({ startLine: 27, startColumn: 6, endLine: 27, endColumn: 46 });
    await waitFor(() => {
      expect(document.querySelector('.peek-row.focus[data-line="27"]')).not.toBeNull();
    });

    // Ten lines inserted above it, and compiled: the card refreshes.
    let answer: (view: unknown) => void = () => undefined;
    pendingCard = new Promise((resolve) => {
      answer = resolve;
    });
    act(() => {
      void queryClient.invalidateQueries({ queryKey: engineKey("card", { project: P, key: CREATE }) });
    });
    await waitFor(() => {
      expect(document.querySelector(".peek-row.focus")).toBeNull();
    });

    await act(async () => {
      answer(movedDown(createOrder, 10));
      await pendingCard;
    });
    await waitFor(() => {
      expect(getPeekTarget()?.focus).toEqual({ startLine: 37, startColumn: 6, endLine: 37, endColumn: 46 });
    });
    expect(getPeekTarget()?.follow).toEqual(PAGE);
    expect(selection()).toEqual({ kind: "card", cardId: "create-order", key: IF });
  });

  it("a sheet not opened by View code doesn't follow the selection", async () => {
    const { container } = open();
    act(() => {
      openPeek({ project: P, path: "README.md" });
    });
    fireEvent.click(await step(container, IF));
    expect(getPeekTarget()).toEqual({ project: P, path: "README.md" });
  });

  it("the card's Open source and a step's View code open highlighted source through the same resolver", async () => {
    const { container } = open();
    const card = await drawn(container, '.board-card[data-card-id="create-order"]');
    fireEvent.contextMenu(card);
    fireEvent.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Open source" }));
    expect(peeked()).toEqual({ project: P, path: ORDERS, line: 25, focus: { startLine: 25, endLine: 25 }, follow: PAGE });

    fireEvent.contextMenu(await step(container, ORDER));
    fireEvent.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: /View code/ }));
    expect(getPeekTarget()?.focus).toEqual({ startLine: 32, startColumn: 16, endLine: 32, endColumn: 41 });
  });

  it("Selected's location links open the same source as View code", async () => {
    const { container } = open();
    fireEvent.click(await step(container, IF));
    fireEvent.click(within(selectedSection()).getByRole("button", { name: "lines 27–31" }));
    expect(getPeekTarget()?.focus).toEqual({ startLine: 27, startColumn: 6, endLine: 27, endColumn: 46 });
  });

  it("an entrance's </> opens its declaration evidence; a resolved target's opens that handler's definition, and its name still follows", async () => {
    const { container } = open([{ id: "import", ref: { entrance: ENTRANCE }, at: [0, 0], view: "flow" }]);
    await drawn(container, '.entrance-card[data-card-id="import"] .ecard-handlers');
    const card = await drawn(container, '.entrance-card[data-card-id="import"]');
    fireEvent.click(within(card).getByRole("button", { name: "View code" }));
    expect(peeked()).toEqual({ project: P, path: "unfold.toml", line: 5, focus: { startLine: 5, endLine: 9 }, follow: PAGE });
    expect(selection()).toEqual({ kind: "card", cardId: "import", key: null });

    fireEvent.click(within(card).getByRole("button", { name: "View importOrders's code" }));
    expect(peeked()).toEqual({ project: P, path: "src/jobs/import-orders.ts", line: 6, focus: { startLine: 6, endLine: 6 }, follow: PAGE });

    fireEvent.click(within(card).getByRole("button", { name: "importOrders" }));
    await waitFor(() => {
      expect(store.getOpenBoard(REF)?.document.pages[0]?.cards.some((item) => "block" in item.ref && item.ref.block === IMPORT)).toBe(true);
    });
  });
});
