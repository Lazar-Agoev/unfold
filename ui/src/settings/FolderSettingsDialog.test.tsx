// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { FolderConfig } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { draftOfSettings, editsBetween, relativeToRoot } from "./folderSettings";
import { FolderSettingsDialog } from "./FolderSettingsDialog";
import { openFolderSettings, useSettingsDialogs } from "./store";

const CONFIG: FolderConfig = {
  exists: true,
  text: "version = 1\n",
  error: null,
  settings: {
    languages: null,
    include: [],
    exclude: ["py-orders/legacy/**", "py-orders/tests/**"],
    disabledDetectors: ["celery.task"],
    // As `get_folder_config` reports them: source roots joined onto the project's root.
    pythonProjects: [{ root: "py-orders", sourceRoots: ["py-orders/src", "py-orders/legacy"], version: "3.14" }],
    entrances: [],
  },
};

let engine: FakeEngine | null = null;

afterEach(() => {
  act(() => {
    useSettingsDialogs.setState({ editor: null, removing: null, folderSettings: false });
  });
  engine?.uninstall();
  engine = null;
});

async function open(config: FolderConfig = CONFIG): Promise<{ fake: FakeEngine; dialog: HTMLElement }> {
  const fake = installFakeEngine({ handlers: { get_folder_config: () => config, update_folder_config: () => config } });
  engine = fake;
  renderWithClient(<FolderSettingsDialog />);
  act(() => {
    openFolderSettings();
  });
  return { fake, dialog: await screen.findByRole("dialog") };
}

describe("folder settings edits", () => {
  it("writes source roots relative to the project's root", () => {
    expect(relativeToRoot("py-orders", "py-orders/src")).toBe("src");
    expect(relativeToRoot("py-orders", "py-orders")).toBe(".");
    expect(relativeToRoot(".", "src")).toBe("src");
    expect(relativeToRoot("", "lib/src")).toBe("lib/src");
  });

  it("makes no edits when nothing changed", () => {
    const saved = draftOfSettings(CONFIG.settings ?? (undefined as never));
    expect(editsBetween(saved, saved)).toEqual([]);
  });
});

describe("the Folder Settings dialog", () => {
  it("shows where the settings live and that they're shared", async () => {
    const { dialog } = await open();
    expect(within(dialog).getByText(".unfold/config.toml")).toBeTruthy();
    expect(within(dialog).getByText(/shared with everyone through the repository/i)).toBeTruthy();
  });

  it("changes a Python project's grammar version", async () => {
    const { fake, dialog } = await open();
    const versions = within(dialog).getByRole("radiogroup", { name: "Python grammar for py-orders" });
    expect(within(versions).getByRole("radio", { name: "3.14" }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(within(versions).getByRole("radio", { name: "3.12" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(fake.callsOf("update_folder_config")).toEqual([
        { edits: [{ op: "set_python_project", project: { root: "py-orders", sourceRoots: ["src", "legacy"], version: "3.12" } }] },
      ]);
    });
  });

  it("adds and removes Python projects", async () => {
    const { fake, dialog } = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove Python project py-orders" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Add Python project" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    // A new project needs its folder.
    expect(within(dialog).getByText(/Name the project's folder/)).toBeTruthy();
    expect(fake.callsOf("update_folder_config")).toHaveLength(0);
    fireEvent.change(within(dialog).getByLabelText("Folder"), { target: { value: "services/api" } });
    fireEvent.change(within(dialog).getByLabelText("Source roots"), { target: { value: "src, lib" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(fake.callsOf("update_folder_config")).toEqual([
        {
          edits: [
            { op: "remove_python_project", root: "py-orders" },
            { op: "set_python_project", project: { root: "services/api", sourceRoots: ["src", "lib"], version: "3.14" } },
          ],
        },
      ]);
    });
  });

  it("turns detectors on and off, but never manual", async () => {
    const { fake, dialog } = await open();
    const celery = within(dialog).getByRole<HTMLInputElement>("checkbox", { name: /Celery tasks/ });
    expect(celery.checked).toBe(false);
    fireEvent.click(celery);
    fireEvent.click(within(dialog).getByRole("checkbox", { name: /Express routes/ }));
    const manual = within(dialog).getByRole<HTMLInputElement>("checkbox", { name: /Manual entrances/ });
    expect(manual.checked && manual.disabled).toBe(true);
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(fake.callsOf("update_folder_config")).toEqual([
        {
          edits: [
            { op: "set_detector", detector: "express.route", enabled: false },
            { op: "set_detector", detector: "celery.task", enabled: true },
          ],
        },
      ]);
    });
  });

  it("removes scope patterns as written", async () => {
    const { fake, dialog } = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove py-orders/tests/** from excluded" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(fake.callsOf("update_folder_config")).toEqual([{ edits: [{ op: "remove_scope_pattern", list: "exclude", pattern: "py-orders/tests/**" }] }]);
    });
  });

  it("shows Rust's refusal with its position", async () => {
    const fake = installFakeEngine({
      handlers: {
        get_folder_config: () => CONFIG,
        update_folder_config: () => {
          // eslint-disable-next-line @typescript-eslint/only-throw-error -- the preload rejects with plain data
          throw { code: "invalid_settings", message: ".unfold/config.toml:7:15: duplicate Python source root `py-orders/src`" };
        },
      },
    });
    engine = fake;
    renderWithClient(<FolderSettingsDialog />);
    act(() => {
      openFolderSettings();
    });
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText("Source roots"), { target: { value: "src, ./src" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toContain("Line 7, column 15: duplicate Python source root");
  });

  it("asks to fix an invalid file first", async () => {
    const { dialog } = await open({ ...CONFIG, settings: null, error: { message: ".unfold/config.toml:2:1: unknown field `bogus`", line: 2, column: 1 } });
    expect(within(dialog).getByRole("alert").textContent).toContain("unknown field `bogus`");
    expect(within(dialog).getByRole("button", { name: "Open in Editor" })).toBeTruthy();
  });
});
