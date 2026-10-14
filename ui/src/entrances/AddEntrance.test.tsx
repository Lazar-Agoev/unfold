// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EntrancesView } from "../ipc/generated";
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

function show(view: EntrancesView, handlers: EngineHandlers = {}) {
  engine = installFakeEngine({
    handlers: {
      entrances: () => view,
      update_project_config: () => ({ config: { exists: true, text: "", settings: null, error: null }, diff: "" }),
      ...handlers,
    },
  });
  const anchor = document.createElement("button");
  document.body.append(anchor);
  const onClose = vi.fn();
  const onAdded = vi.fn();
  renderWithClient(<AddEntrance project={P} anchor={anchor} code={CODE} onClose={onClose} onAdded={onAdded} />);
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
      { project: P, edits: [{ op: "set_entrance", app: "Background jobs", entrance: { name: "Retry failed charges", on: "every hour", code: [CODE], about: null } }] },
    ]);
    expect(onClose).toHaveBeenCalled();
  });

  it("takes a new app's name, which set_entrance creates; on may be left out", async () => {
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
        { project: P, edits: [{ op: "set_entrance", app: "Billing", entrance: { name: "Retry failed charges", on: null, code: [CODE], about: null } }] },
      ]);
    });
  });

  it("asks for a new app's name when the file has no apps", async () => {
    const { popover } = show({ resolved: true, apps: [] });
    expect(await within(popover).findByRole("textbox", { name: "App" })).toBeTruthy();
    expect(within(popover).queryByRole("combobox", { name: "App" })).toBeNull();
  });

  it("says why the line couldn't be written, and stays open", async () => {
    const { popover, onClose } = show(TWO_APPS, {
      update_project_config: () => {
        // Rejections are plain {code, message} data, as from the real preload.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "invalid_argument", message: "Shop API already has an entrance named Retry" };
      },
    });
    await waitFor(() => {
      expect(within(popover).getAllByRole("option")).toHaveLength(3);
    });
    fireEvent.change(within(popover).getByRole("textbox", { name: "Name" }), { target: { value: "Retry" } });
    fireEvent.click(within(popover).getByRole("button", { name: "Add" }));
    expect(await within(popover).findByText("Shop API already has an entrance named Retry")).toBeTruthy();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes on Cancel without writing", () => {
    const { fake, popover, onClose } = show(TWO_APPS);
    fireEvent.click(within(popover).getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
    expect(fake.callsOf("update_project_config")).toEqual([]);
  });
});
