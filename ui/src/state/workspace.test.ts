import { beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../ipc/client";
import { engineKey } from "../ipc/queries";
import { projectRow, workspaceView } from "../test/workspace";

const commands = vi.hoisted(() => ({ call: vi.fn(() => Promise.resolve(null)), callStream: vi.fn(), onEngineEvent: vi.fn() }));
vi.mock("../ipc/commands", () => commands);

const { getProjectIndex, projectIndex, selectProject } = await import("./workspace");
const { useWindowStore } = await import("./window");

const ORDERS = "p-orders0000";
const PAYMENTS = "p-payments00";

const workspace = workspaceView([projectRow(ORDERS, "orders"), projectRow(PAYMENTS, "payments")]);

beforeEach(() => {
  queryClient.clear();
  commands.call.mockClear();
});

describe("project index", () => {
  it("builds one index per workspace, and a new one when the workspace changes", () => {
    expect(projectIndex(workspace)).toBe(projectIndex(workspace));
    queryClient.setQueryData(engineKey("workspace", {}), workspace);
    expect(getProjectIndex().byId.get(PAYMENTS)?.name).toBe("payments");
    queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(ORDERS, "orders")]));
    expect(getProjectIndex().byId.has(PAYMENTS)).toBe(false);
  });
});

describe("selection", () => {
  it("is the window's own, and tells Rust which project to keep live", async () => {
    await selectProject(PAYMENTS);
    expect(useWindowStore.getState().selectedProject).toBe(PAYMENTS);
    expect(commands.call).toHaveBeenCalledWith("select_project", { project: PAYMENTS });
  });
});
