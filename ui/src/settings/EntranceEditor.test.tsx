// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ProjectConfig, SettingsEdit } from "../ipc/generated";
import type { DialogRequest } from "../state/overlays";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { copyDiagnostics } from "./diagnostics";
import { EntranceEditorDialog, RemoveEntranceDialog } from "./EntranceEditor";

const PROJECT = "p-1";

const CONFIG: ProjectConfig = {
  exists: true,
  text: "# Shared settings.\nversion = 1\n",
  error: null,
  name: { value: "storefront-api", isDefault: true },
  settings: {
    languages: null,
    include: [],
    exclude: [],
    disabledDetectors: [],
    pythonProjects: [],
    entrances: [
      { index: 0, kind: "command", trigger: "import", method: null, label: null, handler: ["src/jobs/import-orders.ts#importOrders"], startup: null, line: 4 },
      { index: 1, kind: "event", trigger: "SIGTERM", method: null, label: "Shut down", handler: ["src/server.ts#shutdown"], startup: null, line: 9 },
    ],
  },
};

let engine: FakeEngine | null = null;

function install(update: (edits: SettingsEdit[]) => ProjectConfig | Promise<ProjectConfig> = () => CONFIG): FakeEngine {
  engine = installFakeEngine({
    handlers: {
      project_config: () => CONFIG,
      project_summary: () => fixture("project_summary", "project_summary"),
      update_project_config: (args) => update(args.edits),
    },
  });
  return engine;
}

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

type EditorRequest = Extract<DialogRequest, { readonly kind: "entranceEditor" }>;

const ADD: EditorRequest = { kind: "entranceEditor", project: PROJECT, mode: "add" };

/** "Add as entrance…" on a card for `name` in `path`. */
function addFor(path: string, name: string): EditorRequest {
  return { kind: "entranceEditor", project: PROJECT, mode: "add", handler: { path, kind: "function", name, qualifiedName: name, line: 1 } };
}

let closed = 0;

async function openEditor(request: EditorRequest = ADD): Promise<HTMLElement> {
  closed = 0;
  const { rerender } = renderWithClient(
    <EntranceEditorDialog
      request={request}
      onClose={() => {
        closed += 1;
        rerender(<></>);
      }}
    />,
  );
  return await screen.findByRole("dialog");
}

function type(label: string, value: string): void {
  fireEvent.change(screen.getByLabelText(label), { target: { value } });
}

describe("the manual entrance editor", () => {
  it("adds a command with its handler", async () => {
    const fake = install();
    const dialog = await openEditor(addFor("src/cli.ts", "runReport"));
    expect(within(dialog).getByRole("heading", { name: "Add entrance" })).toBeTruthy();
    expect(screen.getByLabelText<HTMLInputElement>("Handler 1").value).toBe("src/cli.ts#runReport");
    expect(within(dialog).queryByLabelText("Method")).toBeNull();
    type("Trigger", "report");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add entrance" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toHaveLength(1);
    });
    expect(fake.callsOf("update_project_config")[0]).toEqual({
      project: PROJECT,
      edits: [{ op: "add_entrance", entrance: { kind: "command", trigger: "report", method: null, label: null, handler: ["src/cli.ts#runReport"], startup: null } }],
    });
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(closed).toBe(1);
  });

  it("offers a method for HTTP only, with handlers in order", async () => {
    const fake = install();
    const dialog = await openEditor();
    fireEvent.click(within(dialog).getByRole("radio", { name: "HTTP" }));
    const method = within(dialog).getByLabelText("Method");
    fireEvent.change(method, { target: { value: "POST" } });
    type("Trigger", "/orders");
    type("Handler 1", "src/routes/orders.ts#createOrder");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add handler" }));
    type("Handler 2", "src/auth.ts#requireUser");
    fireEvent.click(within(dialog).getByRole("button", { name: "Run handler 2 earlier" }));
    // Switching away from HTTP drops the method.
    fireEvent.click(within(dialog).getByRole("radio", { name: "Worker" }));
    expect(within(dialog).queryByLabelText("Method")).toBeNull();
    fireEvent.click(within(dialog).getByRole("radio", { name: "HTTP" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Add entrance" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toHaveLength(1);
    });
    const [edit] = (fake.callsOf("update_project_config")[0]?.["edits"] ?? []) as SettingsEdit[];
    expect(edit).toEqual({
      op: "add_entrance",
      entrance: { kind: "http", trigger: "/orders", method: "POST", label: null, handler: ["src/auth.ts#requireUser", "src/routes/orders.ts#createOrder"], startup: null },
    });
  });

  it("asks a process for its startup instead of handlers", async () => {
    const fake = install();
    const dialog = await openEditor();
    fireEvent.click(within(dialog).getByRole("radio", { name: "Process" }));
    expect(within(dialog).queryByLabelText("Handler 1")).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: "Add entrance" }));
    expect(await within(dialog).findByText(/needs the file it starts from/)).toBeTruthy();
    expect(fake.callsOf("update_project_config")).toHaveLength(0);
    type("Startup", "src/worker.ts");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add entrance" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toHaveLength(1);
    });
    expect(fake.callsOf("update_project_config")[0]).toMatchObject({ edits: [{ entrance: { kind: "process", handler: [], startup: "src/worker.ts" } }] });
  });

  it("shows locator mistakes before saving", async () => {
    const fake = install();
    const dialog = await openEditor();
    type("Handler 1", "src/cli.ts#a.b.c");
    fireEvent.blur(screen.getByLabelText("Handler 1"));
    expect(within(dialog).getByText(/After # comes a name/)).toBeTruthy();
    expect(screen.getByLabelText("Handler 1").getAttribute("aria-invalid")).toBe("true");
    fireEvent.click(within(dialog).getByRole("button", { name: "Add entrance" }));
    expect(fake.callsOf("update_project_config")).toHaveLength(0);
  });

  it("shows Rust's refusal with its line and column, and stays open", async () => {
    install(() => {
      // eslint-disable-next-line @typescript-eslint/only-throw-error -- the preload rejects with plain data
      throw { code: "invalid_settings", message: ".unfold/config.toml:14:11: `src/cli.ts#x` names a file that isn't TypeScript or Python" };
    });
    const dialog = await openEditor(addFor("src/cli.ts", "x"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Add entrance" }));
    const alert = await within(dialog).findByRole("alert");
    expect(alert.textContent).toContain("Line 14, column 11:");
    expect(alert.textContent).toContain("names a file that isn't TypeScript or Python");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("edits the table an Entrances row came from", async () => {
    const fake = install();
    const row = { key: 'entrance:["manual","event","","SIGTERM","src/server.ts#shutdown"]', detector: "manual" as const, title: "EVENT SIGTERM" };
    const dialog = await openEditor({ kind: "entranceEditor", project: PROJECT, mode: "edit", entrance: row });
    expect(within(dialog).getByRole("heading", { name: "Edit entrance" })).toBeTruthy();
    expect(screen.getByLabelText<HTMLInputElement>("Trigger").value).toBe("SIGTERM");
    expect(screen.getByLabelText<HTMLInputElement>("Label").value).toBe("Shut down");
    type("Trigger", "SIGINT");
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toHaveLength(1);
    });
    expect(fake.callsOf("update_project_config")[0]).toMatchObject({
      edits: [{ op: "replace_entrance", index: 1, entrance: { kind: "event", trigger: "SIGINT", label: "Shut down" } }],
    });
  });

  it("removes a manual entrance after asking", async () => {
    const fake = install();
    const entrance = { key: 'entrance:["manual","command","","import","src/jobs/import-orders.ts#importOrders"]', detector: "manual" as const, title: "import" };
    renderWithClient(<RemoveEntranceDialog request={{ kind: "removeEntrance", project: PROJECT, entrance }} onClose={() => undefined} />);
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => {
      expect(fake.callsOf("update_project_config")).toEqual([{ project: PROJECT, edits: [{ op: "remove_entrance", index: 0 }] }]);
    });
  });
});

describe("Copy Diagnostics", () => {
  it("copies Rust's summary with the shell's versions through the main process", async () => {
    const fake = installFakeEngine({ handlers: { diagnostics: () => ({ text: "Unfold diagnostics\nEngine version: 0.1.0\n" }) } });
    engine = fake;
    await copyDiagnostics();
    expect(fake.windowCalls).toHaveLength(1);
    const call = fake.windowCalls[0];
    expect(call?.command).toBe("copy_to_clipboard");
    const text = (call?.args as { text: string }).text;
    expect(text.startsWith("Unfold diagnostics\nApp: ")).toBe(true);
    expect(text).toContain("Engine version: 0.1.0");
  });
});
