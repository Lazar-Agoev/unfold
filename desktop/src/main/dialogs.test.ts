import { homedir } from "node:os";
import { join } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { confirmAgentAddProject } from "./dialogs";
import { dialogs, FakeBrowserWindow, resetFakeElectron } from "./fake-electron";

vi.mock("electron", async () => (await import("./fake-electron")).electronModule);

beforeEach(() => {
  resetFakeElectron();
});

describe("confirmAgentAddProject", () => {
  const parent = new FakeBrowserWindow({ width: 800, height: 600 }) as unknown as Parameters<typeof confirmAgentAddProject>[0];

  it("asks whether the agent may add the folder, shown from the home folder, with Cancel the default", async () => {
    dialogs.response = 0;
    expect(await confirmAgentAddProject(parent, "Claude", join(homedir(), "repos", "payments"))).toBe(true);
    expect(dialogs.shown).toEqual([
      {
        kind: "message",
        options: {
          type: "question",
          message: `Claude wants to add \`${join("~", "repos", "payments")}\` to your workspace.`,
          detail: "Unfold reads the folder and writes only in its .unfold folder.",
          buttons: ["Add", "Cancel"],
          defaultId: 1,
          cancelId: 1,
        },
      },
    ]);
  });

  it("says no when the person cancels, and shows a path outside the home folder as it is", async () => {
    dialogs.response = 1;
    expect(await confirmAgentAddProject(parent, "Codex", "/srv/payments")).toBe(false);
    expect(dialogs.shown[0]?.options["message"]).toBe("Codex wants to add `/srv/payments` to your workspace.");
  });
});
