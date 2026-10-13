import { describe as group, expect, it, vi } from "vitest";
import type { AgentEditRequest, AgentFailure, AgentItemResult, AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import { addNotes, addResources, connect, describe, frameCards, groupSteps, labelSteps, propose } from "./shape";
import { harness, PROJECT } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const BOARD_INFO = { title: "Order placement", revision: "r1" } as const;
const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";
const PAGE: AgentPage = { id: "page-1", title: "Placing an order", defaultView: "diagram", cards: [], groups: [], labels: [], guards: [], links: [] };

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

group("group_steps", () => {
  it("groups a card's steps by lines or keys, and names the statements each group took", async () => {
    const test = app({
      items: [
        {
          kind: "group",
          id: "queue-it",
          status: "created",
          card: "create-order",
          name: "Queue it for a worker, reply 202",
          lines: [28, 30],
          statements: [
            { key: `${CREATE_ORDER}/if[0]/variable:job`, kind: "variable", lines: [28, 28] },
            { key: `${CREATE_ORDER}/if[0]/return[0]`, kind: "return", lines: [30, 30] },
          ],
        },
      ],
      warnings: ["“Queue it for a worker, reply 202” took whole statements: lines 28–30 of createOrder, not lines 28–29."],
    });
    const { text } = await test.call(groupSteps, { card: `unfold://ts-orders/${BOARD.id}?page=page-1&item=create-order`, groups: [{ title: "Queue it for a worker, reply 202", lines: [28, 29], collapsed: true }] });
    expect(sentOps(test)).toEqual([{ op: "group_steps", card: "create-order", groups: [{ title: "Queue it for a worker, reply 202", lines: [28, 29], collapsed: true }] }]);
    expect(text.split("\n")).toEqual([
      HEADER,
      "Step groups (1 created):",
      "- “Queue it for a worker, reply 202” (step group id queue-it; created) in card create-order, lines 28–30: variable (line 28), return (line 30)",
      "“Queue it for a worker, reply 202” took whole statements: lines 28–30 of createOrder, not lines 28–29.",
      IDS,
      "Next: label_steps for its steps (those inside the groups too), set_view to collapse groups if the card is still long, then check.",
    ]);
    expect((await test.call(groupSteps, { card: "create-order", groups: [{ title: "Both", lines: [1, 2], steps: ["x"] }] })).text).toBe("Each group takes lines or steps, not both.");
  });

  it("doesn't tell the agent to pass replacePeopleText, which it doesn't take", async () => {
    const test = app({
      ok: false,
      code: "people_text",
      message: "Can't change the step group “Checks”: Lazar wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change.",
    });
    expect((await test.call(groupSteps, { card: "create-order", groups: [{ id: "checks", title: "Validate", lines: [26, 26] }] })).text).toBe(
      "Can't change the step group “Checks”: Lazar wrote it. People's words are safe; group_steps keeps them: pass a step group's title and description as they are, or ask the person to change them.",
    );
  });
});

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
});

group("add_resources", () => {
  it("passes resource cards and their anchors as given, and the save's grounding refusal as it is", async () => {
    const test = app({ items: [{ kind: "resource", id: "orders", status: "created", resource: "table", name: "orders", type: "src/types.ts#interface:Order", columns: [{ name: "id", key: "primary" }] }] });
    const { text } = await test.call(addResources, {
      resources: [
        {
          kind: "table",
          name: "orders",
          type: "src/types.ts#interface:Order",
          columns: [{ name: "id", key: "primary" }],
          anchors: [{ key: "src/db/repository.ts#class:OrderRepository" }, { file: "db/schema.sql", lines: [3, 9] }],
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
            type: "src/types.ts#interface:Order",
            columns: [{ name: "id", key: "primary" }],
            anchors: [{ key: "src/db/repository.ts#class:OrderRepository" }, { file: "db/schema.sql", lines: [3, 9] }],
          },
        ],
      },
    ]);
    expect(text).toContain("- table “orders” (resource id orders; created): backed by Order (key src/types.ts#interface:Order); columns id (primary)");

    const refusal = "page `page-1`, resource `orders`: Can't add the table \"orders\": the column \"customer_id\" doesn't appear in its anchors (OrderRepository in src/db/repository.ts). Cite where the column is defined, or leave it out.";
    const grounded = app({ ok: false, code: "board_problem", message: refusal });
    expect(await grounded.call(addResources, { resources: [{ kind: "table", name: "orders", columns: [{ name: "customer_id" }], anchors: [{ key: "src/db/repository.ts#class:OrderRepository" }] }] })).toMatchObject({
      isError: true,
      text: refusal,
    });
  });
});

group("connect", () => {
  it("draws connections and says the channel the save filled in and the type carried", async () => {
    const test = app({
      items: [{ kind: "link", id: "enqueue-order", status: "created", from: "create-order", to: "orders-worker", name: "place-order job", connection: "queue", step: `${CREATE_ORDER}/if[0]/variable:job/call[0]`, channel: "orders", carries: "src/types.ts#interface:OrderJob" }],
    });
    const { text } = await test.call(connect, {
      connections: [
        {
          kind: "queue",
          from: "create-order",
          step: `${CREATE_ORDER}/if[0]/variable:job/call[0]`,
          to: "orders-worker",
          carries: "src/types.ts#interface:OrderJob",
          label: "place-order job",
          description: "createOrder queues the order for the orders worker.",
          anchors: [{ key: "src/queues/queues.ts#variable:ordersQueue" }],
        },
      ],
    });
    expect(text).toContain(
      `- queue connection create-order → orders-worker (link id enqueue-order; created): from step ${CREATE_ORDER}/if[0]/variable:job/call[0], channel orders, carries OrderJob (key src/types.ts#interface:OrderJob), label “place-order job”`,
    );
    expect((await test.call(connect, { connections: [{ kind: "writes", from: "a", to: "b" }] })).isError).toBe(true);
  });
});

group("add_notes, describe, and label_steps", () => {
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

  it("label a card's steps", async () => {
    const test = app({ items: [{ kind: "label", status: "created", card: "create-order", step: `${CREATE_ORDER}/if[0]`, name: "Did the client ask for an async reply?" }] });
    const { text } = await test.call(labelSteps, { card: "create-order", labels: [{ step: `${CREATE_ORDER}/if[0]`, text: "Did the client ask for an async reply?" }] });
    expect(sentOps(test)).toEqual([{ op: "label_steps", card: "create-order", labels: [{ step: `${CREATE_ORDER}/if[0]`, text: "Did the client ask for an async reply?" }] }]);
    expect(text).toContain(`- “Did the client ask for an async reply?” on step ${CREATE_ORDER}/if[0] of card create-order (created)`);
  });
});

group("propose", () => {
  const AFTER = `${CREATE_ORDER}/variable:order`;

  it("sends one card's proposed steps, link and null reason included, and says they are intentions, not code", async () => {
    const test = app({
      items: [
        {
          kind: "proposal",
          id: "email-a-confirmation",
          status: "created",
          card: "create-order",
          after: AFTER,
          title: "Email a confirmation",
          reason: "The queued path sends one; the direct path doesn't.",
          afterLabel: "Place the order",
          afterLine: 32,
        },
        { kind: "proposal", id: "notify", status: "updated", card: "create-order", after: `${CREATE_ORDER}/if[0]`, path: "no", title: "Notify the customer" },
      ],
    });
    const { text } = await test.call(propose, {
      card: `unfold://ts-orders/${BOARD.id}?page=page-1&item=create-order`,
      proposals: [
        { after: AFTER, title: "  Email a confirmation ", reason: "The queued path sends one; the direct path doesn't." },
        { id: "notify", after: `${CREATE_ORDER}/if[0]`, path: "no", reason: null },
      ],
    });
    expect(sentOps(test)).toEqual([
      {
        op: "propose",
        card: "create-order",
        proposals: [
          { after: AFTER, title: "Email a confirmation", reason: "The queued path sends one; the direct path doesn't." },
          { id: "notify", after: `${CREATE_ORDER}/if[0]`, path: "no", reason: null },
        ],
      },
    ]);
    expect(text.split("\n")).toEqual([
      HEADER,
      "Proposed steps (1 created, 1 updated):",
      `- proposed step “Email a confirmation” (proposal id email-a-confirmation; created) in card create-order, after the step “Place the order” (line 32), key ${AFTER}; reason “The queued path sends one; the direct path doesn't.”`,
      `- proposed step “Notify the customer” (proposal id notify; updated) in card create-order, after the step ${CREATE_ORDER}/if[0], on its “no” path`,
      "Proposed steps are intentions, not code: the compiled path stays as it is.",
      IDS,
      "Next: get_screenshot of the card (item create-order), then tell the person in chat what you proposed and why.",
    ]);
    expect(propose.annotations).toEqual({ readOnlyHint: false, destructiveHint: false, idempotentHint: true });
  });

  it("refuses a path that isn't one, and a title past 120 characters, before it reaches the window", async () => {
    const test = app({ items: [] });
    expect((await test.call(propose, { card: "create-order", proposals: [{ after: AFTER, title: "x", path: "maybe" }] })).isError).toBe(true);
    expect((await test.call(propose, { card: "create-order", proposals: [{ after: AFTER, title: "x".repeat(121) }] })).isError).toBe(true);
    expect(sentOps(test)).toBeUndefined();
  });

  it("passes on the window's refusal to change a person's words, with replacePeopleText to pass", async () => {
    const test = app({
      ok: false,
      code: "people_text",
      message: "Can't change the title of the proposed step “Email them”: Maya wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change.",
    });
    const { text, isError } = await test.call(propose, { card: "create-order", proposals: [{ id: "email", title: "Email a confirmation" }] });
    expect(isError).toBe(true);
    expect(text).toContain("pass replacePeopleText only when the person asked for that change.");
  });
});
