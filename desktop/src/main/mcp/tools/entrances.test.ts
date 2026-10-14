import { describe, expect, it, vi } from "vitest";
import type { EntryEdit } from "../engine-types";
import { removeEntrance, setApp, setEntrance } from "./entrances";
import { harness, PROJECT, refusal } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const DIFF = "--- unfold.toml\n+++ unfold.toml\n@@ -1 +1 @@\n-a\n+b\n";

/** A project whose engine says `update_project_config` did `entry`, with `diff`. */
function app(entry: EntryEdit, diff = DIFF) {
  return harness({
    engine: {
      update_project_config: () => ({ config: { exists: true, settings: null, text: "", error: null }, diff, entries: [entry] }),
    },
  });
}

/** The edits each `update_project_config` sent. */
function sent(test: ReturnType<typeof harness>): unknown[] {
  return test.engineCalls.filter(([command]) => command === "update_project_config").map(([, args]) => args["edits"]);
}

/** The commands the tool sent besides the edit: none, so nothing is read back or matched here. */
function others(test: ReturnType<typeof harness>): string[] {
  return test.engineCalls.map(([command]) => command).filter((command) => command !== "update_project_config" && command !== "workspace");
}

const SHARED = "unfold.toml is shared through the repository: write apps and entrances only when the person asks, and tell them the file changed. Unfold looks up the code keys again without compiling.";

describe("set_app", () => {
  it("adds an app, and answers with the change as a diff, saying the file is shared and nothing recompiles", async () => {
    const test = app({ change: "added", app: "Mailer", newApp: true });
    const { text } = await test.call(setApp, { project: PROJECT, app: "Mailer", run: "npm run mailer", code: "src/mailer.ts#" });
    expect(sent(test)).toEqual([[{ op: "set_app", app: { name: "Mailer", run: "npm run mailer", code: "src/mailer.ts#" } }]]);
    expect(text.split("\n")).toEqual([
      "Unfold · ts-orders · as Claude",
      "Added the app Mailer. Changed unfold.toml of ts-orders:",
      "```diff",
      ...DIFF.trimEnd().split("\n"),
      "```",
      SHARED,
      "Next: list_entrances shows whether each code is found, with a fix when one is missing.",
    ]);
    expect(test.commands()).not.toContain("compile");
  });

  it("sends only the fields given, null to remove one, and says what the engine changed", async () => {
    const test = app({ change: "changed", app: "Server", newApp: false });
    const changed = await test.call(setApp, { project: PROJECT, app: "Server!", about: "The HTTP API." });
    expect(changed.text.split("\n")[1]).toBe("Changed the app Server. Changed unfold.toml of ts-orders:");
    await test.call(setApp, { project: PROJECT, app: "entrance:server", run: null });
    expect(sent(test)).toEqual([[{ op: "set_app", app: { name: "Server!", about: "The HTTP API." } }], [{ op: "set_app", app: { name: "entrance:server", run: null } }]]);
    expect(Object.keys((sent(test)[0] as [{ app: object }])[0].app)).toEqual(["name", "about"]);
    expect(others(test)).toEqual([]);
  });

  it("says when nothing changed, and passes the engine's refusal on", async () => {
    expect((await app({ change: "changed", app: "Server", newApp: false }, "").call(setApp, { project: PROJECT, app: "Server", run: "npm run start" })).text.split("\n")[1]).toBe(
      "Nothing in unfold.toml changed: it already said that.",
    );
    const refused = harness({
      engine: {
        update_project_config: () => {
          throw refusal("settings_conflict", "no app “entrance:mailer” is in unfold.toml. Its apps: Server, Worker");
        },
      },
    });
    expect(await refused.call(setApp, { project: PROJECT, app: "entrance:mailer", code: "src/mailer.ts#" })).toMatchObject({
      isError: true,
      text: "No app “entrance:mailer” is in unfold.toml. Its apps: Server, Worker.",
    });
  });
});

describe("set_entrance", () => {
  it("adds an entrance to an app, or to a new app, with one code key or several in order", async () => {
    const added = app({ change: "added", app: "Server", entrance: "Refund an order", newApp: false });
    const one = await added.call(setEntrance, { project: PROJECT, app: "Server", name: "Refund an order", on: "POST /orders/{id}/refund", code: "src/routes/orders.ts#refundOrder" });
    expect(one.text.split("\n")[1]).toBe("Added the entrance Refund an order to Server. Changed unfold.toml of ts-orders:");
    const created = app({ change: "added", app: "Mailer", entrance: "Send emails", newApp: true });
    const many = await created.call(setEntrance, { project: PROJECT, app: "Mailer", name: "Send emails", on: "queue emails", code: ["src/mailer.ts#auth", "src/mailer.ts#sendEmails"] });
    expect(many.text.split("\n")[1]).toBe("Added the entrance Send emails to the new app Mailer. Changed unfold.toml of ts-orders:");
    expect([...sent(added), ...sent(created)]).toEqual([
      [{ op: "set_entrance", app: "Server", entrance: { name: "Refund an order", on: "POST /orders/{id}/refund", code: ["src/routes/orders.ts#refundOrder"] } }],
      [{ op: "set_entrance", app: "Mailer", entrance: { name: "Send emails", on: "queue emails", code: ["src/mailer.ts#auth", "src/mailer.ts#sendEmails"] } }],
    ]);
  });

  it("changes an entrance by name or key, sending only the fields given, and says what the engine changed", async () => {
    const test = app({ change: "changed", app: "Server", entrance: "Status", newApp: false });
    const changed = await test.call(setEntrance, { project: PROJECT, app: "server", name: "entrance:server/status", about: "Says the API is up." });
    expect(changed.text.split("\n")[1]).toBe("Changed the entrance Status of Server. Changed unfold.toml of ts-orders:");
    await test.call(setEntrance, { project: PROJECT, app: "Server", name: "status", on: null });
    expect(sent(test)).toEqual([
      [{ op: "set_entrance", app: "server", entrance: { name: "entrance:server/status", about: "Says the API is up." } }],
      [{ op: "set_entrance", app: "Server", entrance: { name: "status", on: null } }],
    ]);
    expect(Object.keys((sent(test)[1] as [{ entrance: object }])[0].entrance)).toEqual(["name", "on"]);
    expect(others(test)).toEqual([]);
  });

  it("passes on the engine's refusal of a new entrance without code", async () => {
    const test = harness({
      engine: {
        update_project_config: () => {
          throw refusal("settings_conflict", "“Refund an order” is a new entrance of Server, and a new entrance needs code: the block it starts at");
        },
      },
    });
    expect((await test.call(setEntrance, { project: PROJECT, app: "Server", name: "Refund an order" })).text).toBe(
      "“Refund an order” is a new entrance of Server, and a new entrance needs code: the block it starts at.",
    );
    expect(sent(test)).toEqual([[{ op: "set_entrance", app: "Server", entrance: { name: "Refund an order" } }]]);
  });
});

describe("remove_entrance", () => {
  it("removes an entrance, or a whole app, by the name or key given", async () => {
    const one = app({ change: "removed", app: "Server", entrance: "Status", newApp: false });
    expect((await one.call(removeEntrance, { project: PROJECT, app: "server", entrance: "entrance:server/status" })).text.split("\n")[1]).toBe(
      "Removed the entrance Status from Server. Changed unfold.toml of ts-orders:",
    );
    const all = app({ change: "removed", app: "Server", newApp: false });
    expect((await all.call(removeEntrance, { project: PROJECT, app: "entrance:server" })).text.split("\n")[1]).toBe("Removed the app Server and its entrances. Changed unfold.toml of ts-orders:");
    expect([...sent(one), ...sent(all)]).toEqual([[{ op: "remove_entrance", app: "server", entrance: "entrance:server/status" }], [{ op: "remove_entrance", app: "entrance:server", entrance: null }]]);
    expect([...others(one), ...others(all)]).toEqual([]);
  });

  it("passes on the engine's refusal of what the file doesn't declare, which lists what it does", async () => {
    const test = harness({
      engine: {
        update_project_config: () => {
          throw refusal("settings_conflict", "the app Server has no entrance “Refund”. Its entrances: Health check, Status");
        },
      },
    });
    expect((await test.call(removeEntrance, { project: PROJECT, app: "Server", entrance: "Refund" })).text).toBe("The app Server has no entrance “Refund”. Its entrances: Health check, Status.");
  });
});
