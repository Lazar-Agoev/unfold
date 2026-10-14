// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EntrancesView, Key, SearchResults } from "../ipc/generated";
import { installFakeEngine, type EngineHandlers, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { AddEntrance } from "./AddEntrance";

const P = "p-shop000000";
const CODE = "shop/tasks.py#retry_failed_charges";

let engine: FakeEngine | null = null;

const TWO_APPS: EntrancesView = {
  resolved: true,
  apps: [
    { key: "entrance:shop-api", name: "Shop API", entrances: [] },
    { key: "entrance:background-jobs", name: "Background jobs", entrances: [] },
  ],
};

function show(view: EntrancesView, handlers: EngineHandlers = {}, code: Key | null = CODE) {
  engine = installFakeEngine({
    handlers: {
      entrances: () => view,
      update_project_config: () => ({ config: { exists: true, text: "", settings: null, error: null }, diff: "", entries: [] }),
      ...handlers,
    },
  });
  const anchor = document.createElement("button");
  document.body.append(anchor);
  const onClose = vi.fn();
  const onAdded = vi.fn();
  renderWithClient(code === null ? <AddEntrance project={P} anchor={anchor} onClose={onClose} onAdded={onAdded} /> : <AddEntrance project={P} anchor={anchor} code={code} onClose={onClose} onAdded={onAdded} />);
  return { fake: engine, onClose, onAdded, popover: screen.getByRole("dialog", { name: "Add as entrance" }) };
}

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

describe("Add as entrance", () => {
  it("asks for the app, the name, and on, with the card's key in the footer", async () => {
    const { popover } = show(TWO_APPS);
    expect(within(popover).getByText("Add as entrance")).toBeTruthy();
    expect(document.activeElement).toBe(within(popover).getByRole("textbox", { name: "Name" }));
    const app = within(popover).getByRole("combobox", { name: "App" });
    await waitFor(() => {
      expect([...app.querySelectorAll("option")].map((option) => option.textContent)).toEqual(["Shop API", "Background jobs", "New app…"]);
    });
    expect((app as HTMLSelectElement).value).toBe("Shop API");
    const footer = popover.querySelector(".add-entrance-footer code");
    expect(footer?.textContent).toBe(CODE);
    expect(footer?.getAttribute("title")).toBe(CODE);
  });

  it("writes one line into the app picked", async () => {
    const { fake, popover, onClose, onAdded } = show(TWO_APPS);
    const add = within(popover).getByRole("button", { name: "Add" });
    expect((add as HTMLButtonElement).disabled).toBe(true);
    await waitFor(() => {
      expect(within(popover).getAllByRole("option")).toHaveLength(3);
    });
    fireEvent.change(within(popover).getByRole("combobox", { name: "App" }), { target: { value: "Background jobs" } });
    fireEvent.change(within(popover).getByRole("textbox", { name: "Name" }), { target: { value: " Retry failed charges " } });
    fireEvent.change(within(popover).getByRole("textbox", { name: "On" }), { target: { value: "every hour" } });
    fireEvent.click(add);
    await waitFor(() => {
      expect(onAdded).toHaveBeenCalledWith("Background jobs", "Retry failed charges");
    });
    expect(fake.callsOf("update_project_config")).toEqual([
      { project: P, edits: [{ op: "add_entrance", app: "Background jobs", entrance: { name: "Retry failed charges", code: [CODE], on: "every hour" } }] },
    ]);
    expect(onClose).toHaveBeenCalled();
  });

  it("takes a new app's name, which add_entrance creates; on may be left out", async () => {
    const { fake, popover } = show(TWO_APPS);
    await waitFor(() => {
      expect(within(popover).getAllByRole("option")).toHaveLength(3);
    });
    fireEvent.change(within(popover).getByRole("combobox", { name: "App" }), { target: { value: "\u0000new" } });
    fireEvent.change(within(popover).getByRole("textbox", { name: "App" }), { target: { value: "Billing" } });
    fireEvent.change(within(popover).getByRole("textbox", { name: "Name" }), { target: { value: "Retry failed charges" } });
    fireEvent.click(within(popover).getByRole("button", { name: "Add" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toEqual([
        { project: P, edits: [{ op: "add_entrance", app: "Billing", entrance: { name: "Retry failed charges", code: [CODE] } }] },
      ]);
    });
  });

  it("asks for a new app's name when the file has no apps", async () => {
    const { popover } = show({ resolved: true, apps: [] });
    expect(await within(popover).findByRole("textbox", { name: "App" })).toBeTruthy();
    expect(within(popover).queryByRole("combobox", { name: "App" })).toBeNull();
  });

  it("shows the engine's refusal of a name the app has, stays open, and clears it once the name changes", async () => {
    const TAKEN = "Shop API already has the entrance “Retry”; give this one another name";
    const { popover, onClose, onAdded } = show(TWO_APPS, {
      update_project_config: () => {
        // Rejections are plain {code, message} data, as from the real preload.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "settings_conflict", message: TAKEN };
      },
    });
    await waitFor(() => {
      expect(within(popover).getAllByRole("option")).toHaveLength(3);
    });
    const name = within(popover).getByRole("textbox", { name: "Name" });
    fireEvent.change(name, { target: { value: "retry!" } });
    fireEvent.click(within(popover).getByRole("button", { name: "Add" }));
    expect(await within(popover).findByText(TAKEN)).toBeTruthy();
    expect(onClose).not.toHaveBeenCalled();
    expect(onAdded).not.toHaveBeenCalled();
    fireEvent.change(name, { target: { value: "Retry charges" } });
    expect(within(popover).queryByText(TAKEN)).toBeNull();
  });

  it("drops the code picked once its field is edited", async () => {
    const results: SearchResults = {
      query: "retry",
      definitions: {
        hits: [{ project: P, key: CODE, name: "retry_failed_charges", qualifiedName: "retry_failed_charges", path: "shop/tasks.py", kind: "function", line: 3, app: null, score: 600 }],
        total: 1,
        truncated: false,
      },
      entrances: { hits: [], total: 0, truncated: false },
      files: { hits: [], total: 0, truncated: false },
    };
    const { fake, popover } = show(TWO_APPS, { search: () => results }, null);
    await waitFor(() => {
      expect(within(popover).getAllByRole("option")).toHaveLength(3);
    });
    fireEvent.change(within(popover).getByRole("textbox", { name: "Name" }), { target: { value: "Retry failed charges" } });
    const field = within(popover).getByRole("combobox", { name: "Code" });
    fireEvent.change(field, { target: { value: "retry" } });
    fireEvent.click(await within(popover).findByRole("option", { name: "function retry_failed_charges, shop/tasks.py" }));
    const add = within(popover).getByRole<HTMLButtonElement>("button", { name: "Add" });
    expect(popover.querySelector(".add-entrance-footer code")?.textContent).toBe(CODE);
    expect(add.disabled).toBe(false);
    fireEvent.change(field, { target: { value: "retry_failed_charges_v2" } });
    expect(popover.querySelector(".add-entrance-footer code")?.textContent).toBe("");
    expect(add.disabled).toBe(true);
    fireEvent.click(add);
    expect(fake.callsOf("update_project_config")).toEqual([]);
  });

  it("closes on Cancel without writing", () => {
    const { fake, popover, onClose } = show(TWO_APPS);
    fireEvent.click(within(popover).getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
    expect(fake.callsOf("update_project_config")).toEqual([]);
  });
});
