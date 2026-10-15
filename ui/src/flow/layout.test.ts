import { flowBlocks } from "@unfold/desktop/drawing";
import { describe, expect, it } from "vitest";
import type { DrawnFlow, Key, Route, Step } from "../ipc/generated";
import { callbackText, layoutFlow, type FlowGeometry, type FlowPiece, type FlowWire, type Measure, type Rect } from "./layout";

/** The reader's drawn flows of the nine examples and the shaped `place_order`, as the IPC sends them. */
const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

function drawn(name: string): DrawnFlow {
  const flow = GOLDENS[`../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return structuredClone(flow);
}

const NAMES = Object.keys(GOLDENS).map((path) => path.replace(/^.*\//, "").replace(/\.json$/, ""));
const measure: Measure = (text) => text.length * 7;

function lay(flow: DrawnFlow, listed: ReadonlySet<Key> = new Set()): FlowGeometry {
  return layoutFlow(flow, { measure, listed });
}

function pieces<K extends FlowPiece["kind"]>(geometry: FlowGeometry, kind: K): Extract<FlowPiece, { kind: K }>[] {
  return geometry.pieces.filter((piece): piece is Extract<FlowPiece, { kind: K }> => piece.kind === kind);
}

function call(geometry: FlowGeometry, callee: string) {
  const found = pieces(geometry, "call").find((piece) => piece.box.callee === callee);
  if (!found) {
    throw new Error(`no call box ${callee}`);
  }
  return found;
}

function exitOf(geometry: FlowGeometry, key: Key) {
  const found = pieces(geometry, "exit").find((piece) => piece.exit.key === key);
  if (!found) {
    throw new Error(`no exit ${key}`);
  }
  return found;
}

function noteTexts(geometry: FlowGeometry): string[] {
  return pieces(geometry, "note").map((piece) => piece.text);
}

/** The wire standing for a route, matched by its ends and kind. */
function wireOf(geometry: FlowGeometry, from: number, kind: Route["kind"], to: Route["to"]): FlowWire | undefined {
  return geometry.wires.find((wire) => wire.routes.some((route) => route.from === from && route.kind === kind && JSON.stringify(route.to) === JSON.stringify(to)));
}

function inside(inner: Rect, outer: Rect): boolean {
  return inner.x >= outer.x && inner.y >= outer.y && inner.x + inner.width <= outer.x + outer.width + 0.001 && inner.y + inner.height <= outer.y + outer.height + 0.001;
}

function stepById(flow: DrawnFlow, id: number): Step {
  const found = flow.steps.steps.find((step) => step.id === id);
  if (!found) {
    throw new Error(`no step ${id}`);
  }
  return found;
}

/** The routes of a flow and of every flow opened in place inside it. */
function allRoutes(value: unknown): Route[] {
  if (Array.isArray(value)) {
    return value.flatMap(allRoutes);
  }
  if (typeof value !== "object" || value === null) {
    return [];
  }
  return Object.entries(value).flatMap(([field, item]) => (field === "routes" ? (item as Route[]) : allRoutes(item)));
}

/** The keys the card header draws rather than the flow: what runs when each definition is defined. */
function headerKeys(flow: DrawnFlow): Set<Key> {
  return new Set(flowBlocks(flow).flatMap((block) => (block.key === flow.header.key ? flow.header.definedWith.map((item) => item.key) : [])));
}

describe("layoutFlow on the reader's drawn flows", () => {
  it.each(NAMES)("%s: reports every drawn block's box by key, inside the body", (name) => {
    const flow = drawn(name);
    const geometry = lay(flow);
    const skipped = headerKeys(flow);
    for (const block of flowBlocks(flow)) {
      if (!skipped.has(block.key)) {
        expect(geometry.blocks.has(block.key), block.key).toBe(true);
      }
    }
    const body = { x: 0, y: 0, width: geometry.width, height: geometry.height };
    for (const piece of geometry.pieces) {
      expect(inside(piece.rect, body), `${piece.kind} inside the body`).toBe(true);
    }
  });

  it.each(NAMES)("%s: is deterministic and draws a wire only for routes", (name) => {
    const flow = drawn(name);
    expect(lay(flow)).toEqual(lay(flow));
    const routes = allRoutes(flow);
    for (const wire of lay(flow).wires) {
      expect(wire.points.length).toBeGreaterThanOrEqual(2);
      for (const route of wire.routes) {
        expect(routes).toContainEqual(route);
      }
    }
  });

  it("starts with the start pill, ends with the End pill, and joins the steps in order", () => {
    const flow = drawn("01-place-order");
    const geometry = lay(flow);
    const start = pieces(geometry, "start")[0];
    const end = pieces(geometry, "end")[0];
    expect(start?.label).toBe("place_order");
    expect(geometry.blocks.get(flow.header.key)).toBe(start?.rect);
    expect(end && start && end.rect.y > start.rect.y).toBe(true);
    // load_order follows the start; the wire between them stands for route 0 → 1.
    const wire = wireOf(geometry, 0, "on", { step: 1 });
    expect(wire?.points).toEqual([
      { x: 42, y: 42 },
      { x: 42, y: 62 },
    ]);
    expect(call(geometry, "load_order").rect.y).toBe(62);
  });
});

describe("call boxes", () => {
  it("draws a project call with → binds, a chevron that opens it in place, and an anchor to its callee", () => {
    const geometry = lay(drawn("01-place-order"));
    const load = call(geometry, "load_order");
    expect(load.box.binds).toBe("order");
    expect(load.box.actions.open).toBe(true);
    expect(load.path).toEqual(["shop/service.py#place_order/call:load_order"]);
    // Head row (20) and meta row (16): 1 + 5 + 20 + 1 + 16 + 6 + 1.
    expect(load.rect.height).toBe(50);
    // A box with no meta row is shorter.
    expect(call(geometry, "save_order").rect.height).toBe(33);
    const anchor = geometry.anchors.find((item) => item.call === load.box.key);
    expect(anchor).toMatchObject({ to: "shop/store.py#load_order", rect: load.rect, at: { x: load.rect.x + load.rect.width, y: load.rect.y + 16 } });
  });

  it("marks AWAIT, and draws a SOMETIMES call dashed", () => {
    expect(call(lay(drawn("02-place-order-ts")), "loadOrder").box).toMatchObject({ awaited: true, binds: "order" });
    expect(call(lay(drawn("05-route-message")), "fetch_profile").box.sometimes).toBe(true);
  });

  it("marks the call each nested call feeds, in the order they run", () => {
    const geometry = lay(drawn("06-checkout"));
    const order = ["charge_card", "build_receipt", "save_order"].map((name) => call(geometry, name));
    expect(order.map((piece) => piece.box.feeds?.callee ?? null)).toEqual(["build_receipt", "save_order", null]);
    expect(order[0] && order[1] && order[0].rect.y < order[1].rect.y).toBe(true);
  });

  it("lists library and unknown calls written inside a box as rows, each reported by key", () => {
    const flow = drawn("01-place-order");
    const load = stepById(flow, 1);
    if (load.kind !== "call") {
      throw new Error("step 1 is a call");
    }
    load.calls = [
      {
        key: "k/json.dumps",
        callee: "json.dumps",
        form: "call",
        to: { outcome: "library", module: "python:json", note: "python:json" },
        awaited: false,
        sometimes: false,
        inside: [{ key: "k/x", callee: "x.y", form: "call", to: { outcome: "unknown", note: "receiver type unknown" }, awaited: false, sometimes: false, inside: [], actions: { after: false, start: false, open: false } }],
        actions: { after: false, start: false, open: false },
      },
    ];
    const geometry = lay(flow);
    const piece = call(geometry, "load_order");
    expect(piece.rows.map((row) => [row.call.callee, row.depth])).toEqual([
      ["json.dumps", 0],
      ["x.y", 1],
    ]);
    // Two 20px rows, 2px apart, under a 10px rule.
    expect(piece.rect.height).toBe(50 + 10 + 20 + 2 + 20);
    const row = geometry.blocks.get("k/x");
    expect(row && inside(row, piece.rect)).toBe(true);
  });

  it("draws callbacks as rows in their call's box, opened in place inside it", () => {
    const geometry = lay(drawn("09-orders-route"));
    const post = call(geometry, "ordersRouter.post");
    const [callback] = pieces(geometry, "callback");
    expect(post.box.to.outcome).toBe("unknown");
    expect(callback?.path).toEqual(['src/routes/orders.ts#/call:ordersRouter.post("/orders/:id")/callback[0]']);
    expect(callback && inside(callback.rect, post.rect)).toBe(true);
    // An unknown call has no anchor: no card to point at.
    expect(geometry.anchors.some((anchor) => anchor.call === post.box.key)).toBe(false);
  });

  it("words a callback row by the property it is written as, then its signature", () => {
    const [callback] = pieces(lay(drawn("09-orders-route")), "callback");
    if (!callback) {
      throw new Error("no callback row");
    }
    expect(callbackText(callback.callback)).toBe("async (req, res) =>");
    expect(callbackText({ ...callback.callback, property: "onSuccess" })).toBe("onSuccess async (req, res) =>");
    const { signature: _signature, ...unwritten } = callback.callback;
    expect(callbackText(unwritten)).toBe("callback to ordersRouter.post");
  });
});

describe("thin rows", () => {
  it("folds consecutive library and unknown calls into one row with its count", () => {
    const geometry = lay(drawn("08-api"));
    const [thin] = pieces(geometry, "thin");
    expect(thin?.row.calls.map((item) => item.callee)).toEqual(["FastAPI", "APIRouter", "router.post", "Depends", "app.include_router"]);
    expect(thin?.row.unknown).toBe(2);
    expect(thin?.listed).toBe(false);
    for (const item of thin?.row.calls ?? []) {
      expect(geometry.blocks.get(item.key)).toBe(thin?.rect);
    }
  });

  it("lists a row one call per row when asked, each call on its own line", () => {
    const flow = drawn("08-api");
    const gathered = pieces(lay(flow), "thin")[0];
    const geometry = lay(flow, new Set(["shop/api.py#/call:FastAPI"]));
    const [thin] = pieces(geometry, "thin");
    expect(thin?.listed).toBe(true);
    expect(thin && gathered && thin.rect.height > gathered.rect.height).toBe(true);
    const lines = thin?.row.calls.map((item) => geometry.blocks.get(item.key)?.y);
    expect(new Set(lines).size).toBe(5);
  });

  it("reports the calls written inside a thin row's call", () => {
    const geometry = lay(drawn("06-checkout"));
    expect(geometry.blocks.get("shop/checkout.py#checkout/call:audit.record/call:json.dumps")).toBeDefined();
  });
});

describe("decisions", () => {
  const flow = drawn("01-place-order");
  const geometry = lay(flow);

  it("draws a frame with a test head per part, yes into its lane and no down to the next test", () => {
    const heads = pieces(geometry, "head");
    expect(heads.map((head) => [head.look, head.text])).toEqual([
      ["test", "order is None"],
      ["test", 'order["paid"]'],
    ]);
    const frame = pieces(geometry, "frame").find((piece) => piece.frame.form === "if");
    for (const head of heads) {
      expect(frame && inside(head.rect, frame.rect)).toBe(true);
    }
    const yes = wireOf(geometry, 3, "yes", { step: 4 });
    expect(yes?.label?.text).toBe("yes");
    expect(yes?.points[0]?.y).toBe(yes?.points[1]?.y);
    expect(wireOf(geometry, 3, "no", { step: 5 })?.label?.text).toBe("no");
    // The last test's no goes past the decision: down to the frame's bottom, then on to the try.
    expect(wireOf(geometry, 5, "no", { step: 7 })).toBeDefined();
  });

  it("cuts a test too long for its head short with a visible …, keeping it whole", () => {
    const test = 'not isinstance(value.get("email"), str) or "@" not in value["email"]';
    const long = drawn("01-place-order");
    const decision = long.steps.steps.find((step) => step.kind === "decision");
    const [first, second] = decision?.kind === "decision" ? decision.lanes : [];
    if (!first?.test || !second?.test) {
      throw new Error("place_order's decision has two tests");
    }
    first.test.text = test;
    second.test.text = test;
    second.name = "Is the email missing?";
    const [unnamed, named] = pieces(lay(long), "head");
    expect(unnamed?.text).toBe(test);
    expect(unnamed?.shown.name).toBeNull();
    expect(unnamed?.shown.text).toMatch(/…$/);
    expect(test.startsWith(unnamed?.shown.text.slice(0, -1) ?? "")).toBe(true);
    expect(unnamed?.rect.width).toBeLessThanOrEqual(320);
    // A part's name stays whole; its test is cut.
    expect(named?.shown.name).toBe("Is the email missing?");
    expect(named?.shown.text).toMatch(/…$/);
    expect(named?.rect.width).toBeLessThanOrEqual(320);
    // A test that fits is drawn whole.
    expect(pieces(geometry, "head").map((head) => head.shown.text)).toEqual(["order is None", 'order["paid"]']);
  });

  it("labels a ternary's value and draws its else part as an else head", () => {
    const ternary = lay(drawn("05-route-message"));
    const frame = pieces(ternary, "frame").find((piece) => piece.frame.form === "ternary");
    expect(frame?.frame.binds).toBe("decision");
    expect(pieces(ternary, "head").map((head) => [head.look, head.text])).toContainEqual(["else", "else"]);
  });

  it("notes where an if's part continues when its end leaves the decision", () => {
    expect(noteTexts(lay(drawn("07-cli")))).toContain("↓ continues after the if");
  });

  it("draws a switch's cases on a rail from its subject, with no match continuing after it", () => {
    const switched = lay(drawn("02-place-order-ts"));
    const heads = pieces(switched, "head");
    expect(heads.map((head) => [head.look, head.text])).toEqual([
      ["test", "order.status"],
      ["case", 'case "paid"'],
      ["case", 'case "cancelled"'],
      ["none", "no match"],
    ]);
    expect(wireOf(switched, 2, "case", { step: 3 })?.points).toHaveLength(3);
    expect(wireOf(switched, 2, "default", { step: 7 })).toBeDefined();
    expect(noteTexts(switched)).toContain("↓ continues after the switch");
  });

  it("draws a default case as a case, and a case that falls into the next one", () => {
    const flow = drawn("02-place-order-ts");
    const decision = stepById(flow, 2);
    if (decision.kind !== "decision") {
      throw new Error("step 2 is a decision");
    }
    const [paid, cancelled] = decision.lanes;
    if (!paid || !cancelled) {
      throw new Error("two cases");
    }
    paid.fallsThrough = true;
    cancelled.form = "default";
    delete cancelled.test;
    flow.routes = flow.routes.filter((route) => !(route.from === 2 && route.kind !== "case")).map((route) => (route.from === 2 && stepOf(route) === 5 ? { ...route, kind: "default" } : route));
    flow.routes = flow.routes.map((route) => (route.from === 4 ? { ...route, kind: "on", to: { step: 5 } } : route));
    const geometry = lay(flow);
    expect(pieces(geometry, "head").map((head) => head.text)).toEqual(["order.status", 'case "paid"', "default"]);
    expect(noteTexts(geometry)).toContain("↓ falls into the next case");
    expect(wireOf(geometry, 2, "default", { step: 5 })).toBeDefined();
  });
});

function stepOf(route: Route): number | null {
  return typeof route.to === "object" ? route.to.step : null;
}

describe("loops", () => {
  it("titles the frame by its header, loops back with ↺ next item, and leaves on done", () => {
    const geometry = lay(drawn("01-place-order"));
    const frame = pieces(geometry, "frame").find((piece) => piece.look === "loop");
    expect(frame?.frame.header).toBe('for item in order["items"]');
    expect(noteTexts(geometry)).toContain("↺ next item");
    expect(wireOf(geometry, 14, "done", { step: 17 })?.label?.text).toBe("done");
    expect(geometry.blocks.get("shop/service.py#place_order/for[0]/body")).toBeDefined();
  });

  it("says done on the wire into End when the loop is the last step", () => {
    const flow = drawn("01-place-order");
    flow.steps.steps = flow.steps.steps.filter((step) => step.id !== 17);
    flow.routes = flow.routes.filter((route) => route.from !== 17).map((route) => (route.kind === "done" ? { ...route, to: "end" as const } : route));
    const geometry = lay(flow);
    expect(wireOf(geometry, 14, "done", "end")?.label?.text).toBe("done");
  });

  it("draws continue and break with where they go, a header call in the title, and the loop's else", () => {
    const geometry = lay(drawn("04-reserve-first-free"));
    expect(exitOf(geometry, "booking/slots.py#reserve_first_free/for[0]/body/if[0]/then/continue[0]").destination).toBe("next item");
    expect(exitOf(geometry, "booking/slots.py#reserve_first_free/for[0]/body/if[1]/then/break[0]").destination).toBe("leaves the loop");
    const frame = pieces(geometry, "frame").find((piece) => piece.look === "loop");
    const chip = geometry.blocks.get("booking/slots.py#reserve_first_free/for[0]/call:list_slots");
    expect(frame && chip && inside(chip, frame.rect) && chip.y < frame.rect.y + 40).toBe(true);
    const [orElse] = pieces(geometry, "section");
    expect(orElse?.look).toBe("loop-else");
    expect(frame && orElse && inside(orElse.rect, frame.rect)).toBe(true);
    // The body's last test goes back to the header when it fails.
    expect(noteTexts(geometry)).toContain("↺ next item");
    // The break goes after the loop: the wire into notify_reserved stands for it.
    expect(wireOf(geometry, 9, "break", { step: 12 })).toBeDefined();
  });

  it("enters a comprehension's body through its WHEN test, failing back to the next item", () => {
    const geometry = lay(drawn("05-route-message"));
    const frame = pieces(geometry, "frame").find((piece) => piece.frame.form === "comprehension");
    expect(frame?.frame.binds).toBe("labels");
    const test = pieces(geometry, "head").find((head) => head.text === "is_relevant(part)");
    expect(test?.chips.map((chip) => chip.callee)).toEqual(["is_relevant"]);
    expect(wireOf(geometry, 9, "yes", { step: 10 })?.label?.text).toBe("yes");
    expect(noteTexts(geometry)).toEqual(expect.arrayContaining(["no: next item", "↺ next item"]));
  });

  it("draws a do loop as a loop frame", () => {
    const flow = drawn("01-place-order");
    const loop = stepById(flow, 14);
    if (loop.kind !== "loop") {
      throw new Error("step 14 is a loop");
    }
    loop.form = "do";
    loop.header = "do … while (more(order))";
    const frame = pieces(lay(flow), "frame").find((piece) => piece.look === "loop");
    expect(frame?.frame).toMatchObject({ form: "do", header: "do … while (more(order))" });
  });
});

describe("try", () => {
  it("draws TRY with its EXCEPT lane beside it, a dashed on-error route, then ELSE and FINALLY", () => {
    const geometry = lay(drawn("03-sync-inbox"));
    const sections = pieces(geometry, "section");
    expect(sections.map((section) => section.look)).toEqual(["try", "handler", "else", "finally"]);
    const [tried, handler, orElse, last] = sections;
    if (!tried || !handler || !orElse || !last) {
      throw new Error("four sections");
    }
    expect(handler.rect.x).toBeGreaterThan(tried.rect.x + tried.rect.width);
    expect(handler.rect.y).toBe(tried.rect.y);
    expect(orElse.rect.y).toBeGreaterThan(handler.rect.y + handler.rect.height);
    expect(orElse.rect.width).toBe(last.rect.width);
    const onError = wireOf(geometry, 4, "onError", { step: 6 });
    expect(onError).toMatchObject({ dashed: true, error: true, label: { text: "on error" } });
    // A return in the handler passes through finally on its way to End.
    expect(exitOf(geometry, "inbox/sync.py#sync_inbox/with[0]/body/try[0]/except[0]/return[0]").destination).toBe("finally, then End");
  });

  it("draws the project calls in a handler's clause as chips in its title", () => {
    const flow = drawn("03-sync-inbox");
    const plain = pieces(lay(flow), "section").find((section) => section.look === "handler");
    const withBlock = stepById(flow, 1);
    const tried = withBlock.kind === "with" ? withBlock.lanes[0]?.steps.steps[0] : undefined;
    const handler = tried?.kind === "try" ? tried.lanes.find((lane) => lane.form === "except") : undefined;
    const lock = withBlock.kind === "with" ? withBlock.headerCalls[0] : undefined;
    if (!handler?.test || !lock) {
      throw new Error("a handler with a clause, and a call to put in it");
    }
    const key = `${handler.key}/call:mailbox_lock`;
    handler.test.calls = [{ ...lock, key }];
    const geometry = lay(flow);
    const section = pieces(geometry, "section").find((piece) => piece.look === "handler");
    const chip = geometry.blocks.get(key);
    expect(section && chip && inside(chip, section.rect) && chip.y < section.rect.y + 40).toBe(true);
    expect(geometry.anchors.some((anchor) => anchor.call === key)).toBe(true);
    expect(section && plain && section.rect.height > plain.rect.height).toBe(true);
  });

  it("draws a with block titled by its header, with its header call as a chip", () => {
    const geometry = lay(drawn("03-sync-inbox"));
    const frame = pieces(geometry, "frame").find((piece) => piece.look === "with");
    expect(frame?.frame.header).toBe("mailbox_lock(account)");
    expect(frame?.frame.headerCalls.map((box) => box.callee)).toEqual(["mailbox_lock"]);
    expect(geometry.anchors.some((anchor) => anchor.call === "inbox/sync.py#sync_inbox/with[0]/call:mailbox_lock")).toBe(true);
  });
});

describe("exits", () => {
  it("labels exits by their value and says where they go, unless a wire into End shows it", () => {
    const flow = drawn("01-place-order");
    const geometry = lay(flow);
    expect(exitOf(geometry, "shop/service.py#place_order/if[0]/then/raise[0]").destination).toBe("raises to the caller");
    expect(exitOf(geometry, "shop/service.py#place_order/if[0]/elif[0]/return[0]").destination).toBe("End");
    expect(exitOf(geometry, "shop/service.py#place_order/try[0]/except[0]/return[0]").destination).toBe("End");
    expect(exitOf(geometry, "shop/service.py#place_order/return[0]").destination).toBeNull();
    const end = pieces(geometry, "end")[0];
    const wire = wireOf(geometry, 17, "return", "end");
    expect(wire?.points.at(-1)?.y).toBe(end?.rect.y);
  });

  it("draws a project call as an exit's value as a chip that opens in place", () => {
    const geometry = lay(drawn("05-route-message"));
    const exit = exitOf(geometry, "mail/route.py#route_message/return[0]");
    const chip = geometry.blocks.get("mail/route.py#route_message/return[0]/call:deliver");
    expect(chip && inside(chip, exit.rect) && chip !== exit.rect).toBe(true);
    expect(geometry.anchors.some((anchor) => anchor.call === "mail/route.py#route_message/return[0]/call:deliver")).toBe(true);
  });

  it("says a return in an inset returns to the caller", () => {
    const geometry = lay(drawn("07-cli"));
    expect(exitOf(geometry, "shop/service.py#place_order/return[0]").destination).toBe("returns");
  });
});

describe("gap ticks", () => {
  it("sits a tick on the wire into its step, taking no layout of its own", () => {
    const geometry = lay(drawn("01-place-order"));
    const gaps = pieces(geometry, "gap");
    expect(gaps.map((gap) => gap.gap.count)).toEqual([1, 1]);
    for (const gap of gaps) {
      const wire = geometry.wires.find((item) => {
        const [top, bottom] = item.points;
        return top !== undefined && bottom?.x === top.x && top.y <= gap.rect.y && bottom.y >= gap.rect.y + gap.rect.height;
      });
      expect(wire, "a wire runs beside it").toBeDefined();
      expect(gap.path).toBe("shop/service.py");
    }
  });

  it("names the control blocks it hides", () => {
    const geometry = lay(drawn("06-checkout"));
    const [gap] = pieces(geometry, "gap");
    expect(gap?.gap.hidden.map((hidden) => hidden.label)).toEqual(["for loop"]);
    expect(geometry.blocks.get(gap?.gap.hidden[0]?.key ?? "")).toBe(gap?.rect);
  });
});

describe("route states", () => {
  it("dims an unreachable step, marks it never runs, and draws no wire into it", () => {
    const flow = drawn("01-place-order");
    stepById(flow, 13).state = "unreachable";
    flow.routes = flow.routes.filter((route) => stepOf(route) !== 13);
    const geometry = lay(flow);
    expect(call(geometry, "save_order").dim).toBe(true);
    expect(noteTexts(geometry)).toContain("never runs");
    expect(geometry.wires.some((wire) => wire.routes.some((route) => stepOf(route) === 13))).toBe(false);
  });

  it("draws a truncated step's may-continue route dashed", () => {
    const flow = drawn("01-place-order");
    stepById(flow, 14).state = "truncated";
    flow.routes.push({ from: 14, kind: "mayContinue", to: { step: 17 }, through: [] });
    const wire = wireOf(lay(flow), 14, "mayContinue", { step: 17 });
    expect(wire).toMatchObject({ dashed: true, error: false, label: { text: "done" } });
  });

  it("says may continue when a truncated step's wire has no other words", () => {
    const flow = drawn("06-checkout");
    stepById(flow, 3).state = "truncated";
    flow.routes = flow.routes.map((route) => (route.from === 3 ? { ...route, kind: "mayContinue" } : route));
    expect(wireOf(lay(flow), 3, "mayContinue", { step: 4 })?.label?.text).toBe("may continue");
  });

  it("draws a truncated part's may-continue route dashed at the part's end", () => {
    const flow = drawn("01-place-order");
    const loop = stepById(flow, 14);
    const body = loop.kind === "loop" ? loop.lanes[0] : undefined;
    if (!body) {
      throw new Error("step 14 is a loop");
    }
    body.state = "truncated";
    flow.routes.push({ from: body.id, kind: "mayContinue", to: { step: 14 }, through: [] });
    expect(wireOf(lay(flow), body.id, "mayContinue", { step: 14 })).toMatchObject({ dashed: true, label: { text: "may continue" } });
    body.steps.steps = [];
    flow.routes = flow.routes.filter((route) => route.from !== 16);
    expect(wireOf(lay(flow), body.id, "mayContinue", { step: 14 })?.dashed).toBe(true);
  });

  it("draws a truncated definition's may-continue route into End dashed", () => {
    const flow = drawn("01-place-order");
    flow.routes.push({ from: 0, kind: "mayContinue", to: "end", through: [] });
    expect(wireOf(lay(flow), 0, "mayContinue", "end")?.dashed).toBe(true);
  });
});

describe("combined steps", () => {
  it("draws a collapsed combined step as one box with its counts and its ways out", () => {
    const geometry = lay(drawn("place-order-shaped"));
    const combined = pieces(geometry, "combined");
    expect(combined.map((piece) => piece.step.title)).toEqual(["Stop if it's missing or already paid", "Mark the order failed", "Save it and reserve stock"]);
    const [stop, , save] = combined;
    expect(stop?.exits.map((exit) => [exit.verb, exit.destination])).toEqual([
      ["raises", "the caller"],
      ["returns", "End"],
    ]);
    expect(geometry.items.get("stop")).toBe(stop?.rect);
    expect(geometry.blocks.get("shop/service.py#place_order/if[0]")).toBe(stop?.rect);
    // What it holds and doesn't draw outlines it: a builtin in its raise, a call in its loop's body.
    expect(geometry.blocks.get("shop/service.py#place_order/if[0]/then/raise[0]/call:ValueError")).toBe(stop?.rect);
    expect(geometry.blocks.get("shop/service.py#place_order/for[0]/body/call:reserve_stock")).toBe(save?.rect);
    // Arrows from the calls it holds leave from it.
    expect(geometry.anchors.filter((anchor) => anchor.rect === save?.rect).map((anchor) => [anchor.call, anchor.to])).toEqual(save?.step.calls.map((item) => [item.key, item.to]));
    expect(pieces(geometry, "start")[0]).toMatchObject({ label: "Place an order", named: true });
  });

  it("draws an open combined step as a frame around its steps", () => {
    const flow = drawn("place-order-shaped");
    const combined = stepById(flow, 2);
    const decision = stepById(drawn("01-place-order"), 2);
    if (combined.kind !== "combined") {
      throw new Error("step 2 is combined");
    }
    combined.collapsed = false;
    combined.steps = { steps: [decision] };
    flow.routes = [...flow.routes.filter((route) => route.from !== 2), ...drawn("01-place-order").routes.filter((route) => [2, 3, 4, 5, 6].includes(route.from))];
    flow.routes = flow.routes.map((route) => (route.from === 5 && route.kind === "no" ? { ...route, to: { step: 8 } } : route));
    const geometry = lay(flow);
    const piece = pieces(geometry, "combined").find((item) => item.step.item === "stop");
    const frame = pieces(geometry, "frame").find((item) => item.frame.form === "if");
    expect(piece?.step.collapsed).toBe(false);
    expect(piece && frame && inside(frame.rect, piece.rect)).toBe(true);
    expect(piece?.exits).toEqual([]);
  });
});

describe("proposed steps", () => {
  it("draws a proposed step where it goes, joined by its routes", () => {
    const flow = drawn("01-place-order");
    flow.steps.steps.splice(1, 0, { id: 99, state: "reached", kind: "proposed", item: "p-receipt", title: "Send a receipt" });
    flow.routes = flow.routes.flatMap((route): Route[] => (route.from === 1 ? [{ ...route, to: { step: 99 } }, { ...route, from: 99 }] : [route]));
    const geometry = lay(flow);
    const [proposed] = pieces(geometry, "proposed");
    expect(proposed?.step.title).toBe("Send a receipt");
    expect(geometry.items.get("p-receipt")).toBe(proposed?.rect);
    expect(wireOf(geometry, 1, "on", { step: 99 })).toBeDefined();
    expect(wireOf(geometry, 99, "on", { step: 2 })).toBeDefined();
  });
});

describe("insets", () => {
  it("lays out calls opened in place three levels deep, each inside the one that opens it", () => {
    const geometry = lay(drawn("07-cli"));
    const insets = pieces(geometry, "inset");
    expect(insets.map((piece) => piece.flow.header.label)).toEqual(["main", "place_order"]);
    const [main, placeOrder] = insets;
    expect(main && placeOrder && inside(placeOrder.rect, main.rect)).toBe(true);
    const mainCall = call(geometry, "main");
    expect(main && main.rect.y > mainCall.rect.y + mainCall.rect.height).toBe(true);
    const load = call(geometry, "load_order");
    expect(load.path).toEqual(['shop/cli.py#/if[0]/then/call:main', "shop/cli.py#main/call:place_order", "shop/service.py#place_order/call:load_order"]);
    expect(placeOrder && inside(load.rect, placeOrder.rect)).toBe(true);
    // An inset has no start or End pill of its own.
    expect(pieces(geometry, "start")).toHaveLength(1);
    expect(pieces(geometry, "end")).toHaveLength(1);
    expect(noteTexts(geometry)).toContain("↩ returns to the caller");
  });
});
