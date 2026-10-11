// @vitest-environment jsdom
import "../test/dom";
import type { AgentServerStatus } from "@unfold/desktop/agent-api";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { Preferences } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { parseAgentPort } from "./AgentsSection";
import { PreferencesDialog } from "./PreferencesDialog";

const PREFERENCES: Preferences = {
  theme: "system",
  editorCommand: null,
  backgroundUpdates: true,
  threadLimit: null,
  keyBindings: {},
  updates: { enabled: true },
  agents: { enabled: false, port: 29180 },
};

const OFF: AgentServerStatus = { state: "off", port: 29180, address: "http://127.0.0.1:29180/mcp", agents: [] };

let engine: FakeEngine | null = null;

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

async function open(agents: Preferences["agents"], status: AgentServerStatus = OFF) {
  const fake = installFakeEngine({
    handlers: {
      get_preferences: () => ({ ...PREFERENCES, agents }),
      set_preferences: ({ preferences }) => preferences,
    },
    windowHandlers: { agent_server_status: () => status },
  });
  engine = fake;
  renderWithClient(<PreferencesDialog onClose={() => undefined} />);
  const dialog = await screen.findByRole("dialog");
  return { fake, dialog, user: userEvent.setup() };
}

const section = (dialog: HTMLElement) => within(dialog).getByRole("region", { name: "Agents" });
const portField = (dialog: HTMLElement) => within(dialog).getByRole("textbox", { name: "Port" });

describe("parseAgentPort", () => {
  it("takes a whole number from 1024 to 65535", () => {
    expect(parseAgentPort(" 29180 ")).toEqual({ ok: true, value: 29180 });
    for (const text of ["", "80", "65536", "2918.5", "port"]) {
      expect(parseAgentPort(text).ok).toBe(false);
    }
  });
});

describe("Preferences › Agents", () => {
  it("is off until the person turns it on, then shows the address and the install commands, and saves", async () => {
    const { dialog, fake, user } = await open({ enabled: false, port: 29180 });
    const agents = section(dialog);
    const toggle = within(agents).getByRole("switch", { name: "Let agents connect" });
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(within(agents).queryByRole("textbox", { name: "Port" })).toBeNull();

    await user.click(toggle);
    expect(within(agents).getByText("Agents can connect once you save.")).toBeTruthy();
    const commands = within(agents).getByRole("group", { name: "Connect an agent" });
    expect([...commands.querySelectorAll("code")].map((code) => code.textContent)).toEqual([
      "http://127.0.0.1:29180/mcp",
      "claude mcp add --transport http --scope user unfold http://127.0.0.1:29180/mcp",
      "codex mcp add unfold --url http://127.0.0.1:29180/mcp",
    ]);

    await user.click(within(commands).getByRole("button", { name: "Copy the Claude Code command" }));
    expect(fake.windowCalls.filter((call) => call.command === "copy_to_clipboard")).toEqual([
      { command: "copy_to_clipboard", args: { text: "claude mcp add --transport http --scope user unfold http://127.0.0.1:29180/mcp" } },
    ]);

    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(fake.callsOf("set_preferences")).toHaveLength(1);
    });
    expect((fake.callsOf("set_preferences")[0]?.["preferences"] as Preferences).agents).toEqual({ enabled: true, port: 29180 });
  });

  it("lists the connected agents with when each last acted, and follows the server", async () => {
    const listening: AgentServerStatus = {
      ...OFF,
      state: "listening",
      agents: [{ author: "agent:claude", name: "Claude", lastSeenMs: Date.now(), lastActedMs: Date.now() - 5 * 60_000 }],
    };
    const { dialog, fake } = await open({ enabled: true, port: 29180 }, listening);
    const agents = section(dialog);
    expect(await within(agents).findByText("Claude · last acted 5 minutes ago")).toBeTruthy();
    expect(within(agents).getByText(/^Listening\./)).toBeTruthy();

    act(() => {
      fake.emit({
        type: "agent_server_changed",
        status: { ...listening, agents: [{ author: "agent:codex", name: "Codex", lastSeenMs: Date.now(), lastActedMs: null }, ...listening.agents] },
      });
    });
    expect(within(agents).getByText("Codex · connected, hasn't acted yet")).toBeTruthy();

    act(() => {
      fake.emit({ type: "agent_server_changed", status: OFF });
    });
    expect(within(agents).queryByText(/Claude ·/)).toBeNull();
    expect(within(agents).getByText(/^None yet\./)).toBeTruthy();
  });

  it("says when another program has the port, and won't save a port that isn't one", async () => {
    const { dialog, user } = await open({ enabled: true, port: 29180 }, { ...OFF, state: "port_taken" });
    const field = portField(dialog);
    expect(await within(dialog).findByText("Another program uses port 29180. Choose another port, then save.")).toBeTruthy();
    expect(field.getAttribute("aria-invalid")).toBe("true");

    await user.clear(field);
    await user.type(field, "29181");
    expect(within(dialog).queryByText(/Another program uses port/)).toBeNull();
    expect(within(dialog).getByText("Agents can connect once you save.")).toBeTruthy();
    expect(within(section(dialog)).getByText("http://127.0.0.1:29181/mcp")).toBeTruthy();

    await user.clear(field);
    await user.type(field, "80");
    expect(within(dialog).getByText("A whole number from 1024 to 65535.")).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Save" })).toHaveProperty("disabled", true);
  });

  it("saves with agents off whatever the hidden port field holds, keeping the saved port", async () => {
    const { dialog, fake, user } = await open({ enabled: false, port: 29180 });
    const toggle = within(section(dialog)).getByRole("switch", { name: "Let agents connect" });
    await user.click(toggle);
    await user.clear(portField(dialog));
    await user.type(portField(dialog), "abc");
    await user.click(toggle);
    const save = within(dialog).getByRole("button", { name: "Save" });
    expect(save).toHaveProperty("disabled", false);

    await user.click(save);
    await waitFor(() => {
      expect(fake.callsOf("set_preferences")).toHaveLength(1);
    });
    expect((fake.callsOf("set_preferences")[0]?.["preferences"] as Preferences).agents).toEqual({ enabled: false, port: 29180 });
  });
});
