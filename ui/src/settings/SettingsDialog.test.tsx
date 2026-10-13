// @vitest-environment jsdom
import "../test/dom";
import type { AgentServerStatus, ConnectedAgent } from "@unfold/desktop/agent-api";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { SettingsDialog } from "./SettingsDialog";

const ADDRESS = "http://127.0.0.1:29180/mcp";
const LISTENING: AgentServerStatus = { state: "listening", address: ADDRESS, agents: [] };
const CLAUDE: ConnectedAgent = { author: "agent:claude", name: "Claude", lastSeenMs: 1, lastActedMs: null };

let engine: FakeEngine | null = null;

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

async function open(
  saved: AppSettings = {},
  status: AgentServerStatus = LISTENING,
  setSettings: (args: { readonly settings: AppSettings }) => AppSettings = ({ settings }) => settings,
) {
  const fake = installFakeEngine({
    handlers: {
      get_settings: () => saved,
      set_settings: setSettings,
    },
    windowHandlers: { agent_server_status: () => status },
  });
  engine = fake;
  const onClose = vi.fn();
  renderWithClient(<SettingsDialog onClose={onClose} />);
  const dialog = await screen.findByRole("dialog", { name: "Settings" });
  return { fake, dialog, onClose, user: userEvent.setup() };
}

const editorField = (dialog: HTMLElement) => within(dialog).getByRole("textbox", { name: "Editor command" });
const agents = (dialog: HTMLElement) => within(dialog).getByRole("region", { name: "Agents" });

describe("Settings › Editor", () => {
  it("shows the saved command, and saves a new one when the field is left", async () => {
    const { dialog, fake, user } = await open({ editorCommand: "code -g {path}:{line}" });
    const field = editorField(dialog);
    expect((field as HTMLInputElement).value).toBe("code -g {path}:{line}");
    expect(within(dialog).getByText("{path}, {line}, and {column} are replaced. Empty opens files in TextEdit.")).toBeTruthy();
    await user.clear(field);
    await user.type(field, " zed {{path}:{{line} ");
    await user.tab();
    await waitFor(() => {
      expect(fake.callsOf("set_settings")).toEqual([{ settings: { editorCommand: "zed {path}:{line}" } }]);
    });
  });

  it("saves on Enter, and an empty command as none", async () => {
    const { dialog, fake, user } = await open({ editorCommand: "code" });
    await user.clear(editorField(dialog));
    await user.keyboard("{Enter}");
    await waitFor(() => {
      expect(fake.callsOf("set_settings")).toEqual([{ settings: {} }]);
    });
  });

  it("saves what was typed when Escape closes the dialog, once", async () => {
    const { dialog, fake, onClose, user } = await open();
    await user.type(editorField(dialog), "subl");
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      expect(fake.callsOf("set_settings")).toEqual([{ settings: { editorCommand: "subl" } }]);
    });
  });

  it("tries a failed save again when the field is left again", async () => {
    let failures = 1;
    const { dialog, fake, user } = await open({}, LISTENING, ({ settings }) => {
      if (failures > 0) {
        failures -= 1;
        // Rejections are plain {code, message} data, as from the real preload.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "io", message: "Disk full." };
      }
      return settings;
    });
    await user.type(editorField(dialog), "subl");
    await user.tab();
    await waitFor(() => {
      expect(fake.callsOf("set_settings")).toHaveLength(1);
    });
    await user.click(editorField(dialog));
    await user.tab();
    await waitFor(() => {
      expect(fake.callsOf("set_settings")).toEqual([{ settings: { editorCommand: "subl" } }, { settings: { editorCommand: "subl" } }]);
    });
  });

  it("saves nothing when nothing changed", async () => {
    const { dialog, fake, onClose, user } = await open({ editorCommand: "subl" });
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(fake.callsOf("set_settings")).toEqual([]);
  });
});

describe("Settings › Agents", () => {
  it("shows the Claude Code and Codex commands for the server's address, each with a copy button", async () => {
    const { dialog, fake, user } = await open();
    const region = agents(dialog);
    expect(within(region).getByText("Run one in a terminal, once. Agents reach Unfold while it's open.")).toBeTruthy();
    expect([...region.querySelectorAll("code")].map((code) => code.textContent)).toEqual([
      `claude mcp add --transport http --scope user unfold ${ADDRESS}`,
      `codex mcp add unfold --url ${ADDRESS}`,
    ]);
    await user.click(within(region).getByRole("button", { name: "Copy the Codex command" }));
    expect(fake.windowCalls.filter((call) => call.command === "copy_to_clipboard").map((call) => call.args)).toEqual([{ text: `codex mcp add unfold --url ${ADDRESS}` }]);
  });

  it("says nothing until an agent connects, then names it, following the server", async () => {
    const { dialog, fake } = await open();
    const region = agents(dialog);
    await waitFor(() => {
      expect(fake.windowCalls.some((call) => call.command === "agent_server_status")).toBe(true);
    });
    expect(within(region).queryByRole("status")).toBeNull();
    act(() => {
      fake.emit({ type: "agent_server_changed", status: { ...LISTENING, agents: [CLAUDE] } });
    });
    const status = within(region).getByRole("status");
    expect(status.textContent).toBe("Claude Code connected");
    expect(status.querySelector(".app-settings-dot")).not.toBeNull();
  });

  it("says when another program holds the port, and why the server failed", async () => {
    const taken = await open({}, { state: "port_taken", address: ADDRESS, agents: [] });
    const status = await within(agents(taken.dialog)).findByRole("status");
    expect(status.textContent).toBe("Another program holds port 29180, so agents can't reach Unfold.");
    expect(status.querySelector(".app-settings-dot")).toBeNull();
    act(() => {
      taken.fake.emit({ type: "agent_server_changed", status: { state: "failed", address: ADDRESS, message: "Permission denied.", agents: [] } });
    });
    expect(within(agents(taken.dialog)).getByRole("status").textContent).toBe("Permission denied.");
  });
});
