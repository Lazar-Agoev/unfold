// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { BoardDocument, DrawnFlow, Page, SearchResults, TypeDefinition } from "../ipc/generated";
import * as store from "../state/boards/store";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page } from "../test/workspace";
import { AddResource } from "./AddResource";
import { CombineSteps } from "./CombineSteps";
import { ProposeStep } from "./ProposeStep";

const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

function drawn(name: string): DrawnFlow {
  const flow = GOLDENS[`../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return structuredClone(flow);
}

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const PLACE = "shop/service.py#place_order";
const LOAD_CALL = `${PLACE}/call:load_order`;
const SUM_CALL = `${PLACE}/call:sum`;
const DECISION = `${PLACE}/if[0]`;
const TRY = `${PLACE}/try[0]`;
const SAVE_CALL = `${PLACE}/call:save_order`;
const RETURN = `${PLACE}/return[0]`;
const THEN = `${PLACE}/if[0]/then`;
const ORDER = "shop/models.py#Order";
const AUTHOR = "person:maya";

const ORDER_TYPE: TypeDefinition = {
  key: ORDER,
  kind: "class",
  name: "Order",
  path: "shop/models.py",
  line: 4,
  language: "python",
  members: [
    { name: "id", kind: "property", optional: false, readonly: false, default: false, type: { text: "str", spans: [] } },
    { name: "total", kind: "property", optional: false, readonly: false, default: false, type: { text: "int", spans: [] } },
  ],
  inherited: [],
  uses: 3,
};

const FOUND: SearchResults = {
  query: "Order",
  definitions: { hits: [{ project: P, key: ORDER, name: "Order", qualifiedName: "Order", path: "shop/models.py", kind: "class", line: 4, app: null, score: 600 }], total: 1, truncated: false },
  entrances: { hits: [], total: 0, truncated: false },
  files: { hits: [], total: 0, truncated: false },
};

let engine: FakeEngine;
/** What the next save is refused with, if anything. */
let refusal: string | null;

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  refusal = null;
  engine = installFakeEngine({
    handlers: {
      app_info: () => ({ username: "maya", engineVersion: "1" }),
      save_board: ({ board }) => {
        if (refusal !== null) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "invalid_board", message: refusal };
        }
        return boardState(P, ID, boardDocument(board.pages as BoardDocument["pages"]));
      },
      search: () => FOUND,
      type_definition: () => ORDER_TYPE,
    },
  });
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [{ id: "place", ref: { block: PLACE }, at: [0, 0], view: "expanded" }])])));
});

afterEach(() => {
  engine.uninstall();
});

function show(children: ReactNode) {
  const anchor = document.createElement("div");
  document.body.append(anchor);
  return { anchor, ...render(<QueryClientProvider client={queryClient}>{children}</QueryClientProvider>) };
}

function shownPage(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("no page");
  }
  return found;
}

/** Waits until who is signing is known (the action's button is enabled once there is something to do). */
async function ready(dialog: HTMLElement, name: string): Promise<HTMLElement> {
  const button = within(dialog).getByRole("button", { name });
  await waitFor(() => {
    expect(button.hasAttribute("disabled")).toBe(false);
  });
  return button;
}

describe("Combine…", () => {
  function combine(steps: readonly string[] | null, block = LOAD_CALL) {
    let closed = 0;
    const anchor = document.createElement("div");
    document.body.append(anchor);
    render(
      <QueryClientProvider client={queryClient}>
        <CombineSteps target={PAGE} anchor={anchor} flow={drawn("01-place-order")} block={block} steps={steps} onClose={() => (closed += 1)} />
      </QueryClientProvider>,
    );
    return { dialog: screen.getByRole("dialog", { name: "Combine steps" }), closed: () => closed };
  }

  it("lists the body's drawn steps, ticks a consecutive run from the block, and combines it with the steps between, collapsed and selected, as one undo step", async () => {
    const { dialog, closed } = combine(drawn("01-place-order").combinable);
    const options = within(dialog).getAllByRole("option");
    // `sum` is a builtin the card doesn't draw: it isn't listed.
    expect(options.map((option) => option.textContent)).toEqual(["✓load_order", "if", "try", "save_order", 'for item in order["items"]', "return order"]);
    fireEvent.click(within(dialog).getByRole("option", { name: "save_order" }));
    expect(within(dialog).getAllByRole("option", { selected: true }).map((option) => option.textContent.replace("✓", ""))).toEqual(["load_order", "if", "try", "save_order"]);
    expect(within(dialog).getByText("Combine 5 steps")).not.toBeNull();
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Title" }), { target: { value: "Load and charge" } });
    fireEvent.click(await ready(dialog, "Combine"));
    await waitFor(() => {
      expect(shownPage().combined).toEqual([
        { id: "load-and-charge", title: "Load and charge", members: [LOAD_CALL, DECISION, SUM_CALL, TRY, SAVE_CALL], collapsed: true, author: AUTHOR, signature: "" },
      ]);
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "combined", combinedId: "load-and-charge" });
    expect(closed()).toBe(1);
    store.undo(REF);
    expect(shownPage().combined).toEqual([]);
    store.redo(REF);
    expect(shownPage().combined.map((item) => item.id)).toEqual(["load-and-charge"]);
  });

  it("ticks back toward the block, never past it, and needs a title", async () => {
    const { dialog } = combine(drawn("01-place-order").combinable, SAVE_CALL);
    fireEvent.click(within(dialog).getByRole("option", { name: "if" }));
    expect(within(dialog).getAllByRole("option", { selected: true }).map((option) => option.textContent.replace("✓", ""))).toEqual(["if", "try", "save_order"]);
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "Combine" }).hasAttribute("disabled")).toBe(true);
    });
  });

  it("combines a whole control block by itself", async () => {
    const { dialog } = combine(null, DECISION);
    expect(within(dialog).getAllByRole("option").map((option) => option.textContent)).toEqual(["✓if"]);
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Title" }), { target: { value: "Stop if missing" } });
    fireEvent.click(await ready(dialog, "Combine"));
    await waitFor(() => {
      expect(shownPage().combined[0]?.members).toEqual([DECISION]);
    });
  });

  it("shows what the board refuses, and leaves the page as it was with no undo step", async () => {
    refusal = "`shop/service.py#place_order/if[0]` is in two combined steps: combined steps never overlap";
    const { dialog, closed } = combine(drawn("01-place-order").combinable);
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Title" }), { target: { value: "Load it" } });
    fireEvent.click(await ready(dialog, "Combine"));
    expect(await within(dialog).findByText(refusal)).not.toBeNull();
    expect(shownPage().combined).toEqual([]);
    expect(store.canUndo(REF)).toBe(false);
    expect(closed()).toBe(0);
  });
});

describe("Add proposed step…", () => {
  function propose(place: { readonly after: string } | { readonly start: string }, label: string) {
    const { anchor } = show(null);
    render(
      <QueryClientProvider client={queryClient}>
        <ProposeStep target={PAGE} anchor={anchor} place={place} label={label} onClose={() => undefined} />
      </QueryClientProvider>,
    );
    return screen.getByRole("dialog", { name: "Add proposed step" });
  }

  it("needs a title, then proposes the step after the block with its reason, selected, as one undo step", async () => {
    const dialog = propose({ after: RETURN }, "return order");
    expect(within(dialog).getByText("After")).not.toBeNull();
    expect(within(dialog).getByText("return order")).not.toBeNull();
    expect(within(dialog).getByRole("button", { name: "Propose" }).hasAttribute("disabled")).toBe(true);
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Title" }), { target: { value: "Email the receipt" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Why" }), { target: { value: "Customers ask for one." } });
    fireEvent.click(await ready(dialog, "Propose"));
    await waitFor(() => {
      expect(shownPage().proposals).toEqual([{ id: "proposal", after: RETURN, title: "Email the receipt", reason: "Customers ask for one.", author: AUTHOR }]);
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "proposal", proposalId: "proposal" });
    store.undo(REF);
    expect(shownPage().proposals).toBeUndefined();
  });

  it("proposes a step at the start of a body, without a reason", async () => {
    const dialog = propose({ start: THEN }, "order is None");
    expect(within(dialog).getByText("At the start")).not.toBeNull();
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Title" }), { target: { value: "Log the miss" } });
    fireEvent.click(await ready(dialog, "Propose"));
    await waitFor(() => {
      expect(shownPage().proposals).toEqual([{ id: "proposal", start: THEN, title: "Log the miss", author: AUTHOR }]);
    });
  });

  it("shows what the board refuses", async () => {
    refusal = "a proposal can't go after `x`";
    const dialog = propose({ after: RETURN }, "return order");
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Title" }), { target: { value: "Retry" } });
    fireEvent.click(await ready(dialog, "Propose"));
    expect(await within(dialog).findByText(refusal)).not.toBeNull();
    expect(shownPage().proposals).toBeUndefined();
  });
});

describe("Add resource…", () => {
  function add() {
    const { anchor } = show(null);
    render(
      <QueryClientProvider client={queryClient}>
        <AddResource target={PAGE} anchor={anchor} at={[120, 340]} onClose={() => undefined} />
      </QueryClientProvider>,
    );
    return screen.getByRole("dialog", { name: "Add resource" });
  }

  it.each([
    ["Queue", "queue"],
    ["Cache", "cache"],
    ["Storage", "storage"],
    ["Service", "external"],
    ["Actor", "actor"],
  ] as const)("adds a %s card where the menu opened, with no citation, selected, as one undo step", async (word, kind) => {
    const dialog = add();
    fireEvent.click(within(dialog).getByRole("button", { name: word }));
    expect(within(dialog).queryByRole("group", { name: "Columns" })).toBeNull();
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "stripe" } });
    fireEvent.click(await ready(dialog, "Add"));
    await waitFor(() => {
      expect(shownPage().resources).toEqual([{ id: "stripe", kind, name: "stripe", anchors: [], at: [120, 340], author: AUTHOR }]);
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "resource", resourceId: "stripe" });
    store.undo(REF);
    expect(shownPage().resources).toBeUndefined();
  });

  it("adds a table with the columns written for it: names, types, and key marks", async () => {
    const dialog = add();
    expect(within(dialog).getByRole("button", { name: "Table" }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "orders" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Column name" }), { target: { value: "id" } });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Column type" }), { target: { value: "uuid" } });
    fireEvent.change(within(dialog).getByRole("combobox", { name: "Key mark of id" }), { target: { value: "primary" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "+ Column" }));
    const [, second] = within(dialog).getAllByRole("textbox", { name: "Column name" });
    if (!second) {
      throw new Error("no second column");
    }
    fireEvent.change(second, { target: { value: "customer_id" } });
    fireEvent.change(within(dialog).getByRole("combobox", { name: "Key mark of customer_id" }), { target: { value: "foreign" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "+ Column" }));
    fireEvent.click(await ready(dialog, "Add"));
    await waitFor(() => {
      expect(shownPage().resources?.[0]).toMatchObject({
        kind: "table",
        name: "orders",
        columns: [
          { name: "id", type: "uuid", key: "primary" },
          { name: "customer_id", key: "foreign" },
        ],
      });
    });
  });

  it("takes a table's columns from the type that declares them (From a type…), with the person's key marks", async () => {
    const dialog = add();
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "orders" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "From a type…" }));
    fireEvent.change(within(dialog).getByRole("combobox", { name: "Type" }), { target: { value: "Order" } });
    fireEvent.click(await within(dialog).findByRole("option", { name: /Order/ }));
    expect(engine.callsOf("search")).toEqual([{ query: "Order", kinds: ["definitions"], definitionKinds: ["type", "class"], limit: 20, projects: [P] }]);
    const mark = await within(dialog).findByRole("combobox", { name: "Key mark of id" });
    expect(within(dialog).getByText("total")).not.toBeNull();
    expect(within(dialog).getByText("From Order")).not.toBeNull();
    fireEvent.change(mark, { target: { value: "primary" } });
    fireEvent.click(await ready(dialog, "Add"));
    await waitFor(() => {
      expect(shownPage().resources?.[0]).toMatchObject({ kind: "table", name: "orders", type: ORDER, columns: [{ name: "id", key: "primary" }] });
    });
  });

  it("shows what the board refuses", async () => {
    refusal = "resource names must be unique";
    const dialog = add();
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: "orders" } });
    fireEvent.click(await ready(dialog, "Add"));
    expect(await within(dialog).findByText(refusal)).not.toBeNull();
    expect(shownPage().resources).toBeUndefined();
  });
});
