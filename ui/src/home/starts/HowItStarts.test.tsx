// @vitest-environment jsdom
import "../../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Toaster } from "../../components";
import { queryClient } from "../../ipc/client";
import type { AppView, CardView, EntrancesView, SearchResults } from "../../ipc/generated";
import { getOpenBoard, resetBoards } from "../../state/boards/store";
import { useShownBoardStore } from "../../state/shownBoard";
import { fixture, installFakeEngine, type EngineHandlers, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { boardState } from "../../test/workspace";
import { HowItStarts } from "./HowItStarts";
import { AGENT_PROMPT } from "./model";

const P = "p-orders0000";
const MOVED = "src/routes/admin.ts#adminStats";

let engine: FakeEngine | null = null;

/** The ts-orders apps: 7 apps and 18 entrances, one whose code moved, and code that isn't analyzed. */
function ordersApps(): EntrancesView {
  return fixture("entrances", "entrances");
}

/** A definition's card, drawn in `path`. */
function definitionAt(key: string, path: string): CardView {
  const card = fixture("card", "card_create_order");
  if (card.type !== "definition") {
    throw new Error("card_create_order draws a definition");
  }
  return { ...card, flow: { ...card.flow, header: { ...card.flow.header, key, path } } };
}

/** One app with these entrances (each `[name, on, code key, state]`). */
function app(name: string, run: string | undefined, entrances: readonly (readonly [string, string, string, NonNullable<AppView["code"]>["state"]])[], code?: AppView["code"]): AppView {
  return {
    key: `entrance:${name.toLowerCase()}`,
    name,
    ...(run !== undefined ? { run } : {}),
    ...(code ? { code } : {}),
    entrances: entrances.map(([entrance, on, key, state], index) => ({
      key: `entrance:${name.toLowerCase()}/${entrance.toLowerCase()}`,
      name: entrance,
      on,
      code: [{ key, ...(state ? { state } : {}) }],
      lines: [10 + index, 10 + index],
    })),
    lines: [1, 20],
  };
}

function start(view: EntrancesView, handlers: EngineHandlers = {}): FakeEngine {
  engine = installFakeEngine({
    handlers: {
      entrances: () => view,
      update_project_config: () => ({ config: { exists: true, text: "", settings: null, error: null }, diff: "", entries: [] }),
      open_in_editor: () => null,
      ...handlers,
    },
    windowHandlers: { copy_to_clipboard: () => null },
  });
  return engine;
}

function home(highlight: number) {
  return (
    <>
      <HowItStarts project={P} highlight={highlight} />
      <Toaster />
    </>
  );
}

function show() {
  return renderWithClient(home(0));
}

const rowOf = (name: string) => {
  const row = screen.getByText(name, { selector: ".home-start-name" }).closest<HTMLElement>(".home-start-row");
  if (!row) {
    throw new Error(`no row ${name}`);
  }
  return row;
};

beforeEach(() => {
  resetBoards();
  queryClient.clear();
  useShownBoardStore.setState({ board: null });
});

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

describe("How it starts", () => {
  it("lists apps in bold and their entrances indented under them, in file order, with run or on in the right column", async () => {
    start(ordersApps());
    show();
    const list = await screen.findByRole("region", { name: "How it starts" });
    await waitFor(() => {
      expect(list.querySelectorAll(".home-start-row")).toHaveLength(25);
    });
    const rows = [...list.querySelectorAll(".home-start-row")].slice(0, 4);
    expect(rows.map((row) => [row.classList.contains("app-row") ? "app" : "entrance", row.querySelector(".home-start-name")?.textContent, row.querySelector(".home-start-command")?.textContent])).toEqual([
      ["app", "Server", "npm run start"],
      ["entrance", "Health check", "GET /health"],
      ["entrance", "Status", "GET /v1/status"],
      ["entrance", "List orders", "GET /orders"],
    ]);
    // A row never takes the window shell's class (`.app`), whose layout would win over the row's grid.
    expect(rows.some((row) => row.classList.contains("app"))).toBe(false);
    // An HTTP method is bold, as the page draws it; anything else is plain.
    expect(rowOf("Health check").querySelector(".home-start-command b")?.textContent).toBe("GET");
    expect(rowOf("Send an email").querySelector(".home-start-command b")).toBeNull();
    // Each app's rows are grouped, in file order.
    expect([...list.querySelectorAll(".home-start-app > .home-start-row.app-row .home-start-name")].map((name) => name.textContent)).toEqual([
      "Server",
      "Dev server",
      "Worker",
      "Scheduler",
      "Orders CLI",
      "Dev CLI",
      "Legacy export",
    ]);
  });

  it("offers a chevron on every row it can open, and copies an app's command", async () => {
    const fake = start(ordersApps());
    show();
    await screen.findByText("Server");
    expect(rowOf("Server").querySelector(".home-start-chevron .icon")).not.toBeNull();
    expect(rowOf("Health check").querySelector(".home-start-chevron .icon")).not.toBeNull();
    // Only an app's command has a copy icon.
    expect(within(rowOf("Health check")).queryByRole("button", { name: "Copy command" })).toBeNull();
    fireEvent.click(within(rowOf("Server")).getByRole("button", { name: "Copy command" }));
    await waitFor(() => {
      expect(fake.windowCalls).toEqual([{ command: "copy_to_clipboard", args: { text: "npm run start" } }]);
    });
    // Copying doesn't trace the row.
    expect(fake.callsOf("new_board")).toEqual([]);
  });

  it("marks a problem with an amber dot, and says what happened on hover", async () => {
    start(ordersApps());
    show();
    await screen.findByText("Admin stats");
    expect(within(rowOf("Admin stats")).getByRole("img", { name: "Needs a look" })).toBeTruthy();
    expect(within(rowOf("Health check")).queryByRole("img", { name: "Needs a look" })).toBeNull();
    // The fix names its file: nothing more is looked up.
    expect(rowOf("Admin stats").querySelector(".home-start-why")?.textContent).toBe("Moved to src/routes/admin.ts");
    expect(engine?.callsOf("card")).toEqual([]);
    expect(rowOf("Build a report").querySelector(".home-start-why")?.textContent).toBe("Not analyzed");
    expect(rowOf("Legacy export").querySelector(".home-start-why")?.textContent).toBe("Not analyzed");
  });

  it("Fix rewrites the entrance's one code key to the one candidate, and nothing else", async () => {
    const fake = start(ordersApps());
    show();
    await screen.findByText("Admin stats");
    fireEvent.click(within(rowOf("Admin stats")).getByRole("button", { name: "Fix" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toEqual([
        { project: P, edits: [{ op: "replace_code", app: "Server", entrance: "Admin stats", index: 0, old: "src/server.ts#adminStats", code: MOVED }] },
      ]);
    });
    expect(fake.callsOf("new_board")).toEqual([]);
  });

  it("says why when unfold.toml changed since the list was shown", async () => {
    const message = "the code of “Admin stats” in Server no longer has `src/server.ts#adminStats` in that place: unfold.toml changed since it was shown";
    start(ordersApps(), {
      update_project_config: () => {
        // Rejections are plain {code, message} data, as from the real preload.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "settings_conflict", message };
      },
    });
    show();
    await screen.findByText("Admin stats");
    fireEvent.click(within(rowOf("Admin stats")).getByRole("button", { name: "Fix" }));
    expect(await screen.findByText(message)).toBeTruthy();
  });

  it("Fix rewrites an app's code key", async () => {
    const view: EntrancesView = {
      resolved: true,
      apps: [app("Shop", "uvicorn shop.api:app", [], { key: "shop/api.py#", state: "missing", fix: { kind: "moved_module", code: "shop/web/api.py#", path: "shop/web/api.py" } })],
    };
    const fake = start(view);
    show();
    await screen.findByText("Shop");
    expect(rowOf("Shop").querySelector(".home-start-why")?.textContent).toBe("Moved to shop/web/api.py");
    fireEvent.click(within(rowOf("Shop")).getByRole("button", { name: "Fix" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toEqual([{ project: P, edits: [{ op: "replace_code", app: "Shop", entrance: null, index: 0, old: "shop/api.py#", code: "shop/web/api.py#" }] }]);
    });
  });

  it("the not analyzed fix opens unfold.toml, where [scope] is set", async () => {
    const fake = start(ordersApps());
    show();
    await screen.findByText("Build a report");
    fireEvent.click(within(rowOf("Build a report")).getByRole("button", { name: "Fix" }));
    await waitFor(() => {
      expect(fake.callsOf("open_in_editor")).toEqual([{ project: P, path: "unfold.toml" }]);
    });
  });

  it("Choose code… searches what can start, and picking a result rewrites that code line", async () => {
    const view: EntrancesView = {
      resolved: true,
      apps: [
        app("Shop", undefined, [
          ["Refund", "POST /refunds", "shop/api.py#refund", "missing"],
          ["Charge", "POST /charges", "shop/api.py#charge/call:stripe", "not_a_start"],
        ]),
      ],
    };
    const results: SearchResults = {
      query: "refund",
      definitions: {
        hits: [{ project: P, key: "shop/refunds.py#refund_order", name: "refund_order", qualifiedName: "refund_order", path: "shop/refunds.py", kind: "function", line: 9, app: null, score: 600 }],
        total: 1,
        truncated: false,
      },
      entrances: { hits: [], total: 0, truncated: false },
      files: { hits: [], total: 0, truncated: false },
    };
    const fake = start(view, { search: () => results });
    show();
    await screen.findByText("Refund");
    expect(rowOf("Refund").querySelector(".home-start-why")?.textContent).toBe("Not found");
    expect(rowOf("Charge").querySelector(".home-start-why")?.textContent).toBe("Not a start");
    expect(within(rowOf("Charge")).getByRole("button", { name: "Choose code…" })).toBeTruthy();
    fireEvent.click(within(rowOf("Refund")).getByRole("button", { name: "Choose code…" }));
    const popover = screen.getByRole("dialog", { name: "Choose code" });
    const field = within(popover).getByRole("combobox", { name: "Code" });
    expect(document.activeElement).toBe(field);
    fireEvent.change(field, { target: { value: "refund" } });
    fireEvent.click(await within(popover).findByRole("option", { name: "function refund_order, shop/refunds.py" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toEqual([
        { project: P, edits: [{ op: "replace_code", app: "Shop", entrance: "Refund", index: 0, old: "shop/api.py#refund", code: "shop/refunds.py#refund_order" }] },
      ]);
    });
    expect(fake.callsOf("search")).toEqual([{ query: "refund", kinds: ["definitions"], definitionKinds: ["module", "class", "function"], limit: 20, projects: [P] }]);
    expect(screen.queryByRole("dialog", { name: "Choose code" })).toBeNull();
  });

  it("shows an entry's own problem, and opens its line in unfold.toml", async () => {
    const view: EntrancesView = {
      resolved: true,
      apps: [{ ...app("Shop", "make run", []), entrances: [{ key: "entrance:shop/x", name: "Refund", code: [{ key: "shop/api.py#refund" }], lines: [7, 7], problem: { kind: "unknown_field", message: "Unknown field `kind`", line: 8 } }] }],
    };
    const fake = start(view);
    show();
    await screen.findByText("Refund");
    expect(rowOf("Refund").querySelector(".home-start-why")?.textContent).toBe("Unknown field `kind`");
    expect(within(rowOf("Refund")).queryByRole("button", { name: "Fix" })).toBeNull();
    fireEvent.click(rowOf("Refund"));
    await waitFor(() => {
      expect(fake.callsOf("open_in_editor")).toEqual([{ project: P, path: "unfold.toml", line: 8 }]);
    });
  });

  it("shows search once the list has more than 15 entrances, filtering by name and on", async () => {
    start(ordersApps());
    show();
    const search = await screen.findByRole("searchbox", { name: "Search entrances" });
    fireEvent.change(search, { target: { value: "queue" } });
    const names = () => [...document.querySelectorAll(".home-start-name")].map((name) => name.textContent);
    expect(names()).toEqual(["Worker", "Send an email", "Process an order", "Build a report"]);
    fireEvent.change(search, { target: { value: "scheduler" } });
    // An app that matches keeps all its entrances.
    expect(names()).toEqual(["Scheduler", "Clean up expired orders", "Report metrics"]);
    fireEvent.change(search, { target: { value: "nothing like it" } });
    expect(screen.getByText("No entrances match.")).toBeTruthy();
  });

  it("has no search with 15 entrances or fewer", async () => {
    start({ resolved: true, apps: [app("Shop", "make run", [["Refund", "POST /refunds", "shop/api.py#refund", "found"]])] });
    show();
    await screen.findByText("Refund");
    expect(screen.queryByRole("searchbox")).toBeNull();
  });

  it("with no apps, offers Add entrance and Copy agent prompt", async () => {
    const fake = start({ resolved: true, apps: [] });
    show();
    expect(await screen.findByText("No entrances yet")).toBeTruthy();
    expect(screen.getByText("Add one from any card, or let an agent write them.")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: "Add entrance" })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Copy agent prompt" }));
    await waitFor(() => {
      expect(fake.windowCalls).toEqual([{ command: "copy_to_clipboard", args: { text: AGENT_PROMPT } }]);
    });
    expect(await screen.findByText("Copied the agent prompt")).toBeTruthy();
    for (const asked of ["routes, workers, schedules, and scripts", "[[app]]", "set_app", "set_entrance", "list_entrances"]) {
      expect(AGENT_PROMPT).toContain(asked);
    }
    fireEvent.click(screen.getByRole("button", { name: "Add entrance" }));
    expect(screen.getByRole("dialog", { name: "Add as entrance" })).toBeTruthy();
  });

  it("shows nothing until the engine has read unfold.toml", async () => {
    const fake = start({ resolved: false, apps: [] });
    const { container } = show();
    await waitFor(() => {
      expect(fake.callsOf("entrances")).toHaveLength(1);
    });
    expect(container.querySelector(".home-starts")).toBeNull();
  });

  it("Add entrance in its header finds the code by name and writes one line", async () => {
    const results: SearchResults = {
      query: "retry",
      definitions: {
        hits: [{ project: P, key: "src/jobs/retry.ts#retryCharges", name: "retryCharges", qualifiedName: "retryCharges", path: "src/jobs/retry.ts", kind: "function", line: 3, app: null, score: 600 }],
        total: 1,
        truncated: false,
      },
      entrances: { hits: [], total: 0, truncated: false },
      files: { hits: [], total: 0, truncated: false },
    };
    const fake = start(ordersApps(), { search: () => results });
    show();
    fireEvent.click(await screen.findByRole("button", { name: "Add entrance" }));
    const popover = screen.getByRole("dialog", { name: "Add as entrance" });
    fireEvent.change(within(popover).getByRole("combobox", { name: "App" }), { target: { value: "Worker" } });
    fireEvent.change(within(popover).getByRole("textbox", { name: "Name" }), { target: { value: "Retry failed charges" } });
    fireEvent.change(within(popover).getByRole("textbox", { name: "On" }), { target: { value: "every hour" } });
    const add = within(popover).getByRole("button", { name: "Add" });
    expect((add as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(within(popover).getByRole("combobox", { name: "Code" }), { target: { value: "retry" } });
    fireEvent.click(await within(popover).findByRole("option", { name: "function retryCharges, src/jobs/retry.ts" }));
    expect(popover.querySelector(".add-entrance-footer code")?.textContent).toBe("src/jobs/retry.ts#retryCharges");
    fireEvent.click(add);
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toEqual([
        { project: P, edits: [{ op: "add_entrance", app: "Worker", entrance: { name: "Retry failed charges", code: ["src/jobs/retry.ts#retryCharges"], on: "every hour" } }] },
      ]);
    });
    expect(screen.queryByRole("dialog", { name: "Add as entrance" })).toBeNull();
  });

  it("clicking an entrance traces it onto a new board named after it", async () => {
    const fake = start(ordersApps(), {
      card: ({ key }) => (key === "entrance:server/place-an-order" ? fixture("card", "card_place_an_order") : definitionAt(key, "src/routes/orders.ts")),
      new_board: ({ project, title }) => boardState(project, ".unfold/boards/place-an-order.json", undefined, { name: title ?? "" }),
      save_board: ({ project, id }) => boardState(project, id),
    });
    show();
    await screen.findByText("Place an order");
    act(() => {
      fireEvent.click(rowOf("Place an order"));
    });
    await waitFor(() => {
      expect(fake.callsOf("new_board")).toEqual([{ project: P, title: "Place an order" }]);
    });
    const board = { project: P, id: ".unfold/boards/place-an-order.json" };
    await waitFor(() => {
      expect(getOpenBoard(board)?.document.pages[0]?.cards).toHaveLength(4);
    });
    expect(useShownBoardStore.getState().board).toEqual(board);
  });

  it("lights up its problems when the Health line asks", async () => {
    start(ordersApps());
    const { rerender, client } = show();
    await screen.findByText("Admin stats");
    expect(document.querySelectorAll(".home-start-row.highlight")).toHaveLength(0);
    vi.useFakeTimers();
    try {
      rerender(<QueryClientProvider client={client}>{home(1)}</QueryClientProvider>);
      expect([...document.querySelectorAll(".home-start-row.highlight .home-start-name")].map((name) => name.textContent)).toEqual(["Admin stats", "Build a report", "Legacy export"]);
      act(() => {
        vi.advanceTimersByTime(1600);
      });
      expect(document.querySelectorAll(".home-start-row.highlight")).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });
});
