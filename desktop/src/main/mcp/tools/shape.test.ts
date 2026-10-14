import { describe as group, expect, it, vi } from "vitest";
import type { AgentEditRequest, AgentFailure, AgentItemResult, AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import { addNotes, addResources, connect, describe, frameCards, propose } from "./shape";
import { harness, PROJECT } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const BOARD_INFO = { title: "Order placement", revision: "r1" } as const;
const CREATE_ORDER = "src/routes/orders.ts#createOrder";
const PAGE: AgentPage = { id: "page-1", title: "Placing an order", defaultView: "compact", cards: [], names: [], combined: [], guards: [], links: [] };

/** A window that holds the board and answers an edit with `answer`. */
function app(answer: { readonly items: readonly AgentItemResult[]; readonly warnings?: readonly string[] } | AgentFailure) {
  const test = harness({
    window: (request: AgentShellEvent) =>
      request.type === "agent_edit" ? ("ok" in answer ? answer : { ok: true, page: PAGE, board: BOARD_INFO, items: answer.items, warnings: answer.warnings ?? [] }) : undefined,
  });
  test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
  return test;
}

function sentOps(test: ReturnType<typeof harness>): AgentEditRequest["ops"] | undefined {
  return test.requests.find((request): request is AgentShellEvent & AgentEditRequest => request.type === "agent_edit")?.ops;
}

const HEADER = `Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(PAGE)} · as Claude`;
const IDS = `Ids: board ${BOARD.id}, page page-1. One undo step in Unfold, saved.`;

group("frame_cards", () => {
  it("frames items and says what each frame holds, nested and collapsed, with the window's warning on shared code", async () => {
    const test = app({
      items: [
        { kind: "frame", id: "server", status: "created", name: "server", look: "box", icon: "api", members: [{ id: "post-orders", kind: "card", name: "POST /orders" }, { id: "request-checks", kind: "frame", name: "Request checks" }] },
        { kind: "frame", id: "request-checks", status: "updated", name: "Request checks", look: "box", collapsed: true, parent: "server", members: [{ id: "require-user", kind: "card", name: "requireUser" }] },
      ],
      warnings: ["OrderService.place is in no frame, and server and worker both call it. Consider a frame of its own (icon library)."],
    });
    const { text } = await test.call(frameCards, {
      frames: [
        { title: "server", members: ["post-orders", "request-checks"], icon: "api" },
        { id: "request-checks", title: "Request checks", members: ["require-user"], parent: "server", collapsed: true },
      ],
    });
    expect(sentOps(test)).toEqual([
      {
        op: "frame_cards",
        frames: [
          { title: "server", members: ["post-orders", "request-checks"], icon: "api" },
          { id: "request-checks", title: "Request checks", members: ["require-user"], parent: "server", collapsed: true },
        ],
      },
    ]);
    expect(text.split("\n").slice(1, 5)).toEqual([
      "Frames (1 created, 1 updated):",
      "- server (frame id server; box, api; created) holds POST /orders (card id post-orders), Request checks (frame id request-checks)",
      "- Request checks (frame id request-checks; box, collapsed, in frame server; updated) holds requireUser (card id require-user)",
      "OrderService.place is in no frame, and server and worker both call it. Consider a frame of its own (icon library).",
    ]);
    expect(text.split("\n").at(-1)).toBe("Next: arrange_page, then check.");
  });

  it("doesn't tell the agent to pass replacePeopleText, which it doesn't take", async () => {
    const test = app({
      ok: false,
      code: "people_text",
      message: "Can't change the frame “Checks”: Lazar wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change.",
    });
    expect((await test.call(frameCards, { frames: [{ id: "checks", title: "Validate", members: ["require-user"] }] })).text).toBe(
      "Can't change the frame “Checks”: Lazar wrote it. People's words are safe; frame_cards keeps them: pass a frame's title and description (you may still change its members, look, and collapse) as they are, or ask the person to change them.",
    );
  });
});

group("add_resources", () => {
  it("passes resource cards and their anchors as given, and the save's grounding refusal as it is", async () => {
    const test = app({ items: [{ kind: "resource", id: "orders", status: "created", resource: "table", name: "orders", type: "src/types.ts#Order", columns: [{ name: "id", key: "primary" }] }] });
    const { text } = await test.call(addResources, {
      resources: [
        {
          kind: "table",
          name: "orders",
          type: "src/types.ts#Order",
          columns: [{ name: "id", key: "primary" }],
          anchors: [{ key: "src/db/repository.ts#OrderRepository" }, { file: "db/schema.sql", lines: [3, 9] }],
        },
      ],
    });
    expect(sentOps(test)).toEqual([
      {
        op: "add_resources",
        resources: [
          {
            kind: "table",
            name: "orders",
            type: "src/types.ts#Order",
            columns: [{ name: "id", key: "primary" }],
            anchors: [{ key: "src/db/repository.ts#OrderRepository" }, { file: "db/schema.sql", lines: [3, 9] }],
          },
        ],
      },
    ]);
    expect(text).toContain("- table “orders” (resource id orders; created): backed by Order (key src/types.ts#Order); columns id (primary)");

    const refusal = "page `page-1`, resource `orders`: Can't add the table \"orders\": the column \"customer_id\" doesn't appear in its anchors (OrderRepository in src/db/repository.ts). Cite where the column is defined, or leave it out.";
    const grounded = app({ ok: false, code: "board_problem", message: refusal });
    expect(await grounded.call(addResources, { resources: [{ kind: "table", name: "orders", columns: [{ name: "customer_id" }], anchors: [{ key: "src/db/repository.ts#OrderRepository" }] }] })).toMatchObject({
      isError: true,
      text: refusal,
    });
  });
});

group("connect", () => {
  it("draws connections and says the channel the save filled in and the type carried", async () => {
    const test = app({
      items: [{ kind: "link", id: "enqueue-order", status: "created", from: "create-order", to: "orders-worker", name: "place-order job", connection: "queue", step: `${CREATE_ORDER}/if[0]/then/call:ordersQueue.add`, channel: "orders", carries: "src/types.ts#OrderJob" }],
    });
    const { text } = await test.call(connect, {
      connections: [
        {
          kind: "queue",
          from: "create-order",
          step: `${CREATE_ORDER}/if[0]/then/call:ordersQueue.add`,
          to: "orders-worker",
          carries: "src/types.ts#OrderJob",
          label: "place-order job",
          description: "createOrder queues the order for the orders worker.",
          anchors: [{ key: "src/queues/queues.ts#/call:Queue" }],
        },
      ],
    });
    expect(text).toContain(
      `- queue connection create-order → orders-worker (link id enqueue-order; created): from step ${CREATE_ORDER}/if[0]/then/call:ordersQueue.add, channel orders, carries OrderJob (key src/types.ts#OrderJob), label “place-order job”`,
    );
    expect((await test.call(connect, { connections: [{ kind: "writes", from: "a", to: "b" }] })).isError).toBe(true);
  });
});

group("add_notes and describe", () => {
  it("add notes, attached or free", async () => {
    const test = app({ items: [{ kind: "note", id: "note-1", status: "created", name: "The client gets a job id, not the order.", attach: "create-order" }] });
    const { text } = await test.call(addNotes, { notes: [{ text: "The client gets a job id, not the order.", attach: "create-order" }] });
    expect(sentOps(test)).toEqual([{ op: "add_notes", notes: [{ text: "The client gets a job id, not the order.", attach: "create-order" }] }]);
    expect(text).toContain("- note “The client gets a job id, not the order.” (note id note-1; created), attached to create-order");
    expect((await test.call(addNotes, { notes: [{ text: "x".repeat(401) }] })).isError).toBe(true);
  });

  it("describe items by id or link, replacing people's words only when asked", async () => {
    const test = app({ items: [{ kind: "card", id: "create-order", status: "updated", key: CREATE_ORDER, name: "createOrder" }] });
    const { text } = await test.call(describe, { items: [{ id: `unfold://ts-orders/${BOARD.id}?page=page-1&item=create-order`, text: "Places the order now." }], replacePeopleText: true });
    expect(sentOps(test)).toEqual([{ op: "describe", items: [{ id: "create-order", text: "Places the order now." }], replacePeopleText: true }]);
    expect(text.split("\n").slice(1, 3)).toEqual(["Described (1 updated):", "- createOrder (card id create-order, updated)"]);
  });
});

group("propose", () => {
  const AFTER = `${CREATE_ORDER}/call:orderService.place`;
  const START = `${CREATE_ORDER}/if[0]/else`;

  it("sends a page's proposed steps, after a block or at the start of a body, null reason included, and says they are intentions, not code", async () => {
    const test = app({
      items: [
        { kind: "proposal", id: "email-a-confirmation", status: "created", after: AFTER, title: "Email a confirmation", reason: "The queued path sends one; the direct path doesn't." },
        { kind: "proposal", id: "notify", status: "updated", start: START, title: "Notify the customer" },
      ],
    });
    const { text } = await test.call(propose, {
      proposals: [
        { after: AFTER, title: "  Email a confirmation ", reason: "The queued path sends one; the direct path doesn't." },
        { id: "notify", start: START, reason: null },
      ],
    });
    expect(sentOps(test)).toEqual([
      {
        op: "propose",
        proposals: [
          { after: AFTER, title: "Email a confirmation", reason: "The queued path sends one; the direct path doesn't." },
          { id: "notify", start: START, reason: null },
        ],
      },
    ]);
    expect(text.split("\n")).toEqual([
      HEADER,
      "Proposed steps (1 created, 1 updated):",
      `- proposed step “Email a confirmation” (proposal id email-a-confirmation; created), after ${AFTER}; reason “The queued path sends one; the direct path doesn't.”`,
      `- proposed step “Notify the customer” (proposal id notify; updated), at the start of ${START}`,
      "Proposed steps are intentions, not code: the compiled path stays as it is.",
      IDS,
      "Next: get_screenshot of the page, then tell the person in chat what you proposed and why.",
    ]);
    expect(propose.annotations).toEqual({ readOnlyHint: false, destructiveHint: false, idempotentHint: true });
  });

  it("refuses both after and start, and a title past 120 characters, before it reaches the window", async () => {
    const test = app({ items: [] });
    expect((await test.call(propose, { proposals: [{ after: AFTER, start: START, title: "x" }] })).isError).toBe(true);
    expect((await test.call(propose, { proposals: [{ after: AFTER, title: "x".repeat(121) }] })).isError).toBe(true);
    expect(sentOps(test)).toBeUndefined();
  });

  it("passes on the window's refusal to change a person's words, with replacePeopleText to pass", async () => {
    const test = app({
      ok: false,
      code: "people_text",
      message: "Can't change the title of the proposed step “Email them”: Maya wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change.",
    });
    const { text, isError } = await test.call(propose, { proposals: [{ id: "email", title: "Email a confirmation" }] });
    expect(isError).toBe(true);
    expect(text).toContain("pass replacePeopleText only when the person asked for that change.");
  });
});
