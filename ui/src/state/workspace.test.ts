import { beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../ipc/client";
import { engineKey } from "../ipc/queries";
import { projectRow, workspaceView } from "../test/workspace";

const commands = vi.hoisted(() => ({ call: vi.fn(() => Promise.resolve(null)), callStream: vi.fn(), onEngineEvent: vi.fn() }));
vi.mock("../ipc/commands", () => commands);

const { getProjectIndex, projectIndex, refProjectFor, resolveRefProject, selectProject } = await import("./workspace");
const { useWindowStore } = await import("./window");

const ORDERS = "p-orders0000";
const PAYMENTS = "p-payments00";
const CLONE = "p-orders2000";

const workspace = workspaceView([projectRow(ORDERS, "orders"), projectRow(PAYMENTS, "payments"), projectRow(CLONE, "orders")]);

beforeEach(() => {
  queryClient.clear();
  commands.call.mockClear();
});

describe("name resolution (D-3)", () => {
  it("resolves a ref's shared name to the first connected project of that name, and no name to the board's project", () => {
    const index = projectIndex(workspace);
    expect(resolveRefProject(index, PAYMENTS, undefined)).toBe(PAYMENTS);
    expect(resolveRefProject(index, PAYMENTS, "orders")).toBe(ORDERS);
    expect(resolveRefProject(index, ORDERS, "payments")).toBe(PAYMENTS);
    expect(resolveRefProject(index, ORDERS, "billing")).toBeNull();
  });

  it("builds one index per workspace, and re-resolves when the workspace changes", () => {
    expect(projectIndex(workspace)).toBe(projectIndex(workspace));
    queryClient.setQueryData(engineKey("workspace", {}), workspace);
    expect(resolveRefProject(getProjectIndex(), ORDERS, "payments")).toBe(PAYMENTS);
    // Payments is removed (or renamed): its cards turn into placeholders.
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(ORDERS, "orders"), projectRow(PAYMENTS, "billing")]));
    expect(resolveRefProject(getProjectIndex(), ORDERS, "payments")).toBeNull();
    expect(getProjectIndex().byId.get(PAYMENTS)?.name).toBe("billing");
  });

  it("names another project in a new ref, omits the board's own, and refuses a same-name project (V-62)", () => {
    const index = projectIndex(workspace);
    expect(refProjectFor(index, ORDERS, ORDERS)).toEqual({ ok: true });
    expect(refProjectFor(index, ORDERS, PAYMENTS)).toEqual({ ok: true, project: "payments" });
    expect(refProjectFor(index, ORDERS, CLONE)).toEqual({ ok: false, reason: "same_name" });
    expect(refProjectFor(index, PAYMENTS, CLONE)).toEqual({ ok: true, project: "orders" });
  });
});

describe("selection", () => {
  it("is the window's own, and tells Rust which project to keep live", async () => {
    await selectProject(PAYMENTS);
    expect(useWindowStore.getState().selectedProject).toBe(PAYMENTS);
    expect(commands.call).toHaveBeenCalledWith("select_project", { project: PAYMENTS });
  });
});
