import { describe, expect, it, vi } from "vitest";
import entrancesView from "../../../../../testdata/workspace/commands/entrances.json";
import type { EntrancesView } from "../engine-types";
import { removeEntrance, setApp, setEntrance } from "./entrances";
import { harness, PROJECT, recorded, refusal } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const DIFF = "--- unfold.toml\n+++ unfold.toml\n@@ -1 +1 @@\n-a\n+b\n";

/** A project whose unfold.toml declares the recorded apps; `update_project_config` answers with `diff`. */
function app(diff = DIFF) {
  return harness({
    engine: {
      entrances: () => recorded<EntrancesView>(entrancesView),
      update_project_config: () => ({ config: { exists: true, settings: null, text: "", error: null }, diff }),
    },
  });
}

/** The edits each `update_project_config` sent. */
function sent(test: ReturnType<typeof harness>): unknown[] {
  return test.engineCalls.filter(([command]) => command === "update_project_config").map(([, args]) => args["edits"]);
}

const SHARED = "unfold.toml is shared through the repository: write apps and entrances only when the person asks, and tell them the file changed. Unfold looks up the code keys again without compiling.";

describe("set_app", () => {
  it("adds an app, and answers with the change as a diff, saying the file is shared and nothing recompiles", async () => {
    const test = app();
    const { text } = await test.call(setApp, { project: PROJECT, app: "Mailer", run: "npm run mailer", code: "src/mailer.ts#" });
    expect(sent(test)).toEqual([[{ op: "set_app", app: { name: "Mailer", run: "npm run mailer", code: "src/mailer.ts#", about: null } }]]);
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

  it("changes an existing app by name or key, keeping the fields left out and removing the ones given null", async () => {
    const test = app();
    await test.call(setApp, { project: PROJECT, app: "entrance:server", about: "The HTTP API." });
    await test.call(setApp, { project: PROJECT, app: "server", run: null });
    expect(sent(test)).toEqual([
      [{ op: "set_app", app: { name: "Server", run: "npm run start", code: "src/server.ts#", about: "The HTTP API." } }],
      [{ op: "set_app", app: { name: "Server", run: null, code: "src/server.ts#", about: null } }],
    ]);
  });

  it("says when nothing changed, and passes the engine's refusal on", async () => {
    expect((await app("").call(setApp, { project: PROJECT, app: "Server", run: "npm run start" })).text.split("\n")[1]).toBe("Nothing in unfold.toml changed: it already said that.");
    const refused = harness({
      engine: {
        entrances: () => recorded<EntrancesView>(entrancesView),
        update_project_config: () => {
          throw refusal("settings_conflict", "an app's name has no letters or digits");
        },
      },
    });
    expect(await refused.call(setApp, { project: PROJECT, app: "!!", code: "src/mailer.ts#" })).toMatchObject({ isError: true, text: "An app's name has no letters or digits." });
  });
});

describe("set_entrance", () => {
  it("adds an entrance to an app, creating the app when it is new, with one code key or several in order", async () => {
    const test = app();
    const added = await test.call(setEntrance, { project: PROJECT, app: "Server", name: "Refund an order", on: "POST /orders/{id}/refund", code: "src/routes/orders.ts#refundOrder" });
    expect(added.text.split("\n")[1]).toBe("Added the entrance Refund an order to Server. Changed unfold.toml of ts-orders:");
    const created = await test.call(setEntrance, { project: PROJECT, app: "Mailer", name: "Send emails", on: "queue emails", code: ["src/mailer.ts#auth", "src/mailer.ts#sendEmails"] });
    expect(created.text.split("\n")[1]).toBe("Added the entrance Send emails to the new app Mailer. Changed unfold.toml of ts-orders:");
    expect(sent(test)).toEqual([
      [{ op: "set_entrance", app: "Server", entrance: { name: "Refund an order", on: "POST /orders/{id}/refund", code: ["src/routes/orders.ts#refundOrder"], about: null } }],
      [{ op: "set_entrance", app: "Mailer", entrance: { name: "Send emails", on: "queue emails", code: ["src/mailer.ts#auth", "src/mailer.ts#sendEmails"], about: null } }],
    ]);
  });

  it("changes an existing entrance by name or key, keeping its code and on when they are left out", async () => {
    const test = app();
    const changed = await test.call(setEntrance, { project: PROJECT, app: "entrance:server", name: "entrance:server/status", about: "Says the API is up." });
    expect(changed.text.split("\n")[1]).toBe("Changed the entrance Status of Server. Changed unfold.toml of ts-orders:");
    await test.call(setEntrance, { project: PROJECT, app: "Server", name: "health check", on: null, code: "src/server.ts#health" });
    expect(sent(test)).toEqual([
      [{ op: "set_entrance", app: "Server", entrance: { name: "Status", on: "GET /v1/status", code: ["src/server.ts#status"], about: "Says the API is up." } }],
      [{ op: "set_entrance", app: "Server", entrance: { name: "Health check", on: null, code: ["src/server.ts#health"], about: null } }],
    ]);
  });

  it("refuses a new entrance without code", async () => {
    const test = app();
    expect((await test.call(setEntrance, { project: PROJECT, app: "Server", name: "Refund an order" })).text).toBe(
      "Can't add the entrance: Refund an order is new, and an entrance needs code: the block it starts at. Pass code: a module's, definition's, or inline handler's key, from search or read_code.",
    );
    expect(sent(test)).toEqual([]);
  });
});

describe("remove_entrance", () => {
  it("removes an entrance, or a whole app, by name or key", async () => {
    const test = app();
    const one = await test.call(removeEntrance, { project: PROJECT, app: "server", entrance: "entrance:server/status" });
    expect(one.text.split("\n")[1]).toBe("Removed the entrance Status from Server. Changed unfold.toml of ts-orders:");
    const all = await test.call(removeEntrance, { project: PROJECT, app: "entrance:server" });
    expect(all.text.split("\n")[1]).toBe("Removed the app Server and its entrances. Changed unfold.toml of ts-orders:");
    expect(sent(test)).toEqual([[{ op: "remove_entrance", app: "Server", entrance: "Status" }], [{ op: "remove_entrance", app: "Server", entrance: null }]]);
  });

  it("refuses an app or entrance the file doesn't declare, listing what it does", async () => {
    const test = app();
    expect((await test.call(removeEntrance, { project: PROJECT, app: "Billing" })).text).toMatch(/^Can't remove it: ts-orders declares no app “Billing”\. Its apps: Server/);
    expect((await test.call(removeEntrance, { project: PROJECT, app: "Server", entrance: "Refund" })).text).toMatch(/^Can't remove the entrance: the app Server has no entrance “Refund”\. Its entrances: Health check, Status, /);
    expect(sent(test)).toEqual([]);
  });
});
