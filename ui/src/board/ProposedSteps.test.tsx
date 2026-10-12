// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, Group, Health, HealthItem, Proposal, RepairBoardArgs } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { closePeek, getPeekTarget } from "../state/overlays";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { HEALTHY, boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardSidebar } from "./BoardSidebar";
import { BoardView } from "./BoardView";

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

const P = "p-orders0000";
const REF = { project: P, id: ".unfold/local/drafts/b.json" };
const PAGE = { board: REF, page: "page-1" };
const ORDERS = "src/routes/orders.ts";
const CREATE = `${ORDERS}#function:createOrder`;
const IF = `${CREATE}/if[0]`;
const RETURN = `${IF}/return[0]`;
const ORDER = `${CREATE}/variable:order`;
const SLACK = `${CREATE}/expression_statement[0]`;
const REPLY = `${CREATE}/expression_statement[1]`;
const GONE = `${CREATE}/variable:gone`;

const createOrder = fixture("card", "card_create_order");
const createCard: Card = { id: "create-order", ref: { block: CREATE }, at: [0, 0], view: "flow" };
const placeGroup: Group = { id: "place-it", card: "create-order", title: "Place it now", members: [ORDER, SLACK, REPLY], collapsed: true };
const email: Proposal = { id: "email", card: "create-order", after: ORDER, title: "Email a confirmation", author: "agent:claude" };

/** The node of the card's own Flow that shows `key`. */
const NODE_IDS: Readonly<Record<string, number>> = { [IF]: 2, [RETURN]: 5, [ORDER]: 6 };

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        if (key === CREATE) {
          return createOrder;
        }
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "not_found", message: "no such key" };
      },
      relations: () => fixture("relations", "relations_create_order"),
      line_labels: () => [],
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: () => boardState(P, REF.id, store.getOpenBoard(REF)?.document, { health: store.getOpenBoard(REF)?.health ?? HEALTHY }),
      save_board_view: () => null,
      repair_board: (args: RepairBoardArgs) => boardState(args.project, args.id, store.getOpenBoard(REF)?.document, { revision: "r2" }),
    },
  });
});

afterEach(() => {
  act(() => {
    closePeek();
  });
  store.resetBoards();
  engine.uninstall();
});

function open(options: { readonly groups?: readonly Group[]; readonly proposals?: readonly Proposal[]; readonly health?: Health } = {}) {
  const document = boardDocument([page("page-1", [createCard], { defaultView: "flow", groups: [...(options.groups ?? [])], ...(options.proposals ? { proposals: [...options.proposals] } : {}) })]);
  store.adoptBoardState(boardState(P, REF.id, document, options.health ? { health: options.health } : {}));
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  queryClient.setQueryData(engineKey("app_info", {}), { username: "maya", engineVersion: "0" });
  return render(
    <QueryClientProvider client={queryClient}>
      <div style={{ width: 1200, height: 800 }}>
        <BoardSidebar board={REF} />
        <BoardView board={REF} />
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

const step = (container: HTMLElement, key: string) => drawn(container, `.block-card [data-flow-node="${String(NODE_IDS[key])}"]`);
const selection = () => store.getPageView(PAGE).selection;
const selectedSection = () => screen.getByRole("region", { name: "Selected" });
const proposalsNow = () => store.getOpenBoard(REF)?.document.pages[0]?.proposals ?? [];
const health = (items: HealthItem[]): Health => ({ ...HEALTHY, items });

describe("Add proposed step after…", () => {
  it("creates it in place after a real step, as the person, selected with its title focused; one undo step removes it", async () => {
    const { container } = open();
    fireEvent.contextMenu(await step(container, ORDER));
    fireEvent.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Add proposed step after…" }));
    expect(proposalsNow()).toEqual([{ id: "proposal", card: "create-order", after: ORDER, title: "Proposed step", author: "person:maya" }]);
    expect(selection()).toEqual({ kind: "proposal", proposalId: "proposal" });
    const title = await within(selectedSection()).findByRole("textbox", { name: "Proposed step title" });
    await waitFor(() => {
      expect(document.activeElement).toBe(title);
    });
    const box = await drawn(container, '.block-card [data-proposal="proposal"]');
    expect(box.classList.contains("selected")).toBe(true);
    expect(within(screen.getByRole("listbox", { name: "Cards on this page" })).getByText("Proposed step")).toBeTruthy();

    act(() => {
      store.undo(REF);
    });
    expect(proposalsNow()).toEqual([]);
  });

  it("offers a decision's actual paths, and nothing after a return or on a combined block", async () => {
    const { container } = open({ groups: [placeGroup] });
    fireEvent.contextMenu(await step(container, IF));
    fireEvent.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Add proposed step after" }));
    const paths = await screen.findByRole("menu", { name: "Add proposed step after" });
    expect(within(paths).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["on yes…", "on no…"]);
    fireEvent.click(within(paths).getByRole("menuitem", { name: "on no…" }));
    expect(proposalsNow()).toEqual([{ id: "proposal", card: "create-order", after: IF, path: "no", title: "Proposed step", author: "person:maya" }]);

    fireEvent.contextMenu(await step(container, RETURN));
    let menu = await screen.findByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: /View code/ })).toBeTruthy();
    expect(within(menu).queryByRole("menuitem", { name: /Add proposed step/ })).toBeNull();
    fireEvent.keyDown(menu, { key: "Escape" });

    fireEvent.contextMenu(await drawn(container, '[data-flow-node="group:place-it"]'));
    menu = await screen.findByRole("menu");
    expect(within(menu).getByRole("menuitem", { name: "Ungroup" })).toBeTruthy();
    expect(within(menu).queryByRole("menuitem", { name: /Add proposed step/ })).toBeNull();
  });
});

describe("Selected for a proposed step", () => {
  it("edits its title and Why in place (each field one undo step, making them yours), says where it goes, and opens the placement code", async () => {
    open({ proposals: [email] });
    act(() => {
      store.select(PAGE, { kind: "proposal", proposalId: "email" });
    });
    const selected = within(selectedSection());
    expect(selected.getByText("PROPOSED STEP")).toBeTruthy();
    expect(selectedSection().querySelector(".selected-byline .agent-author")?.textContent).toMatch(/^Claude · /);
    expect(await selected.findByText("after OrderService.place")).toBeTruthy();

    // View placement code: the step it goes after, never code of the proposal's own.
    fireEvent.click(await selected.findByRole("button", { name: "View placement code, routes/orders.ts:32" }));
    expect(getPeekTarget()).toMatchObject({ project: P, path: ORDERS, line: 32, focus: { startLine: 32, startColumn: 16, endLine: 32, endColumn: 41 }, follow: PAGE });
    // It finds that placement again as the code changes.
    expect(getPeekTarget()?.resolve?.()).toEqual({ project: P, path: ORDERS, line: 32, focus: { startLine: 32, startColumn: 16, endLine: 32, endColumn: 41 } });

    const title = selected.getByRole("textbox", { name: "Proposed step title" });
    fireEvent.change(title, { target: { value: "Email a r" } });
    fireEvent.change(title, { target: { value: "Email a receipt" } });
    expect(proposalsNow()).toEqual([{ ...email, title: "Email a receipt", author: "person:maya" }]);
    // A title is required: a blank one isn't saved.
    fireEvent.change(title, { target: { value: "  " } });
    expect(selected.getByText("A proposed step needs a title.")).toBeTruthy();
    expect(proposalsNow()[0]?.title).toBe("Email a receipt");
    fireEvent.blur(title);
    expect((title as HTMLInputElement).value).toBe("Email a receipt");

    const why = selected.getByRole("textbox", { name: "Why" });
    fireEvent.change(why, { target: { value: "Only the queued path emails one." } });
    expect(proposalsNow()[0]?.reason).toBe("Only the queued path emails one.");
    fireEvent.change(why, { target: { value: "" } });
    expect("reason" in (proposalsNow()[0] ?? {})).toBe(false);

    act(() => {
      store.undo(REF);
    });
    expect(proposalsNow()).toEqual([{ ...email, title: "Email a receipt", author: "person:maya" }]);
    act(() => {
      store.undo(REF);
    });
    expect(proposalsNow()).toEqual([email]);
  });

  it("removes it as one undo step, from Selected or Remove the Selection, never touching the code", async () => {
    const { container } = open({ proposals: [email] });
    await drawn(container, '[data-proposal="email"]');
    fireEvent.click(await drawn(container, '[data-proposal="email"]'));
    expect(selection()).toEqual({ kind: "proposal", proposalId: "email" });
    fireEvent.click(within(selectedSection()).getByRole("button", { name: "Remove proposal" }));
    expect(proposalsNow()).toEqual([]);
    await waitFor(() => {
      expect(container.querySelector('[data-proposal="email"]')).toBeNull();
    });
    act(() => {
      store.undo(REF);
    });
    expect(proposalsNow()).toEqual([email]);
    expect(engine.callsOf("repair_board")).toEqual([]);
  });

  it("is listed under its card in On this page, and selecting the row selects it", async () => {
    open({ proposals: [email] });
    const list = within(screen.getByRole("listbox", { name: "Cards on this page" }));
    const row = (await list.findByText("Email a confirmation")).closest<HTMLElement>(".layer-row");
    expect(row?.classList.contains("proposal")).toBe(true);
    expect(row?.querySelector(".layer-note")?.textContent).toBe("proposed");
    fireEvent.click(row ?? document.body);
    expect(selection()).toEqual({ kind: "proposal", proposalId: "email" });
    expect(row?.getAttribute("aria-selected")).toBe("true");
  });
});

describe("a proposed step whose step is missing or held", () => {
  const missing: HealthItem = {
    page: "page-1",
    kind: "missing",
    target: { proposal: "email" },
    key: GONE,
    message: `The step this proposal goes after is missing: ${GONE}. Reconnect it to the step it goes after now, or remove the proposal`,
    repairs: ["reconnect", "removeProposal"],
  };
  const held: HealthItem = {
    page: "page-1",
    kind: "review",
    target: { proposal: "email" },
    key: ORDER,
    message: "The step this proposal goes after changed kind or owner; check where it goes, then accept the current code, reconnect it to another step, or remove the proposal",
    repairs: ["accept", "reconnect", "removeProposal"],
  };

  it("keeps its words in the missing look with why, and Reconnect moves it after a step you pick, keeping its author", async () => {
    const { container } = open({ proposals: [{ ...email, after: GONE }], health: health([missing]) });
    const box = await drawn(container, '[data-proposal="email"]');
    expect(box.classList.contains("missing")).toBe(true);
    expect(box.textContent).toContain("Email a confirmation");
    expect(box.querySelector(".flow-proposal-note")?.textContent).toBe("The step it went after is missing.");
    expect(container.querySelector(".block-card .card-review-text")?.textContent).toBe("A proposed step lost the step it goes after");

    fireEvent.click(box);
    const selected = within(selectedSection());
    expect(await selected.findByText("after createOrder.gone (missing)")).toBeTruthy();
    fireEvent.click(selected.getByRole("button", { name: "Review" }));
    const popover = within(await screen.findByRole("dialog", { name: "Review" }));
    expect(popover.getByText(/Moves the proposed step "Email a confirmation" to go after a step of its card you choose/)).toBeTruthy();
    const steps = popover.getByRole("listbox", { name: "Goes after" });
    const choices = within(steps).getAllByRole("option").map((option) => option.querySelector(".repair-hit-name")?.textContent);
    // Real steps only, a decision's paths each its own choice; never the return.
    expect(choices).toContain("req.header? · on yes");
    expect(choices).toContain("req.header? · on no");
    expect(choices).toContain("OrderService.place");
    expect(choices).not.toContain("Return");
    fireEvent.click(within(steps).getByRole("option", { name: /req\.header\? · on no/ }));
    fireEvent.click(popover.getAllByRole("button", { name: "Apply" })[0] ?? document.body);
    expect(proposalsNow()).toEqual([{ ...email, after: IF, path: "no" }]);
    expect(engine.callsOf("repair_board")).toEqual([]);
  });

  it("stays in place while held, saying so, and offers Accept and Remove through the board's repairs", async () => {
    const { container } = open({ proposals: [email], health: health([held]) });
    const box = await drawn(container, '[data-proposal="email"]');
    expect(box.classList.contains("held")).toBe(true);
    expect(box.querySelector(".flow-proposal-note")?.textContent).toBe("The step it goes after is held for review.");
    expect(container.querySelector(".block-card .card-review-text")?.textContent).toBe("Where a proposed step goes needs review");
    fireEvent.click(container.querySelector<HTMLElement>(".block-card .card-review-action") ?? document.body);
    const popover = within(await screen.findByRole("dialog", { name: "Review" }));
    expect(popover.getByText(/Accepts the current code of the step the proposed step "Email a confirmation" goes after/)).toBeTruthy();
    expect(popover.getByText(/Removes the proposed step "Email a confirmation". The code it goes after stays as it is./)).toBeTruthy();
    fireEvent.click(popover.getAllByRole("button", { name: "Apply" })[0] ?? document.body);
    await waitFor(() => {
      expect(engine.callsOf("repair_board")).toEqual([{ project: P, id: REF.id, repair: { page: "page-1", kind: "accept", key: ORDER } }]);
    });
  });
});
