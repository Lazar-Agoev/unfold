// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DrawnFlow, Key, OtherCall } from "../ipc/generated";
import { FlowView, type FlowViewProps } from "./FlowView";
import { layoutFlow, type Measure } from "./layout";

const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

function drawn(name: string): DrawnFlow {
  const flow = GOLDENS[`../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return flow;
}

const measure: Measure = (text) => text.length * 7;

function show(name: string, props: Partial<FlowViewProps> = {}, listed: ReadonlySet<Key> = new Set()) {
  const geometry = layoutFlow(drawn(name), { measure, listed });
  const view = render(<FlowView geometry={geometry} {...props} />);
  return { geometry, container: view.container };
}

function all(container: HTMLElement, selector: string): HTMLElement[] {
  return [...container.querySelectorAll<HTMLElement>(selector)];
}

/** A library or unknown call written at `key`. */
function other(key: Key, callee: string, outcome: "library" | "unknown"): OtherCall {
  return {
    key,
    callee,
    form: "call",
    to: outcome === "unknown" ? { outcome, reason: "unknown_receiver" } : { outcome, module: "logging" },
    awaited: false,
    sometimes: false,
    inside: [],
    actions: { after: false, start: false, open: false },
  };
}

/** place_order, with unknown calls written in its first test and its loop's header, and calls written inside its exits' values. */
function withListedCalls(): DrawnFlow {
  const flow = structuredClone(drawn("01-place-order"));
  for (const step of flow.steps.steps) {
    if (step.kind === "decision") {
      const test = step.lanes[0]?.test;
      if (test) {
        test.unknown = [other("p/if[0]/then/call:order.mystery", "order.mystery", "unknown")];
      }
      const raise = step.lanes[0]?.steps.steps[0];
      if (raise?.kind === "exit" && raise.value?.kind === "call") {
        raise.value.calls = [other("p/raise[0]/call:ValueError/call:x.bar", "x.bar", "unknown")];
      }
    }
    if (step.kind === "loop") {
      step.headerUnknown = [other("p/for[0]/call:order.items", "order.items", "unknown")];
    }
    if (step.kind === "try") {
      const back = step.lanes[1]?.steps.steps.find((inner) => inner.kind === "exit");
      if (back?.kind === "exit" && back.value?.kind === "call") {
        back.value.calls = [other("p/return[0]/call:mark_failed/call:x.foo", "x.foo", "unknown"), other("p/return[0]/call:mark_failed/call:logger.info", "logger.info", "library")];
      }
    }
  }
  return flow;
}

describe("FlowView", () => {
  it("lists a test's and a header's unknown calls after them as chips with their `?`, and the calls written inside an exit's value on its pill", () => {
    const onSelect = vi.fn();
    const geometry = layoutFlow(withListedCalls(), { measure });
    const { container } = render(<FlowView geometry={geometry} onSelect={onSelect} />);
    const unknown = all(container, ".flow-chip.other");
    expect(unknown.map((chip) => [chip.dataset["key"], chip.textContent])).toEqual([
      ["p/if[0]/then/call:order.mystery", "order.mystery?"],
      ["p/for[0]/call:order.items", "order.items?"],
    ]);
    expect(all(container, ".flow-head.test")[0]?.textContent).toBe("◆order is Noneorder.mystery?");
    expect(all(container, ".flow-frame.loop .flow-frame-title")[0]?.textContent).toContain('for item in order["items"]order.items?');
    expect(all(container, ".flow-exit.x-raise")[0]?.textContent).toBe("RaiseValueError+ x.bar?→ raises to the caller");
    expect(container.querySelector(".flow-exit .flow-chip .flow-listed")?.textContent).toBe("+ x.foo?, logger.info");
    for (const key of ["p/if[0]/then/call:order.mystery", "p/for[0]/call:order.items", "p/raise[0]/call:ValueError/call:x.bar", "p/return[0]/call:mark_failed/call:x.foo", "p/return[0]/call:mark_failed/call:logger.info"]) {
      expect(geometry.blocks.has(key), key).toBe(true);
    }
    fireEvent.click(screen.getByText("order.mystery"));
    expect(onSelect).toHaveBeenLastCalledWith({ kind: "block", key: "p/if[0]/then/call:order.mystery" });
    const plain = layoutFlow(drawn("01-place-order"), { measure });
    const pill = (laid: typeof geometry) => laid.pieces.find((piece) => piece.kind === "exit" && piece.exit.form === "raise")?.rect.width ?? 0;
    expect(pill(geometry)).toBeGreaterThan(pill(plain));
  });

  it("says a with block's keyword once: its frame word, then its header without the keyword", () => {
    // Its header as written: `with mailbox_lock(account)`.
    const { geometry, container } = show("03-sync-inbox");
    const title = container.querySelector(".flow-frame.with .flow-frame-title");
    expect(title?.querySelector(".flow-frame-word")?.textContent).toBe("With");
    expect(title?.querySelector(".flow-frame-header")?.textContent).toBe("mailbox_lock(account)");
    // The layout measures the same words.
    const flow = structuredClone(drawn("03-sync-inbox"));
    const frame = flow.steps.steps.find((step) => step.kind === "with");
    if (frame?.kind !== "with") {
      throw new Error("03-sync-inbox draws no with block");
    }
    frame.header = "mailbox_lock(account)";
    const width = (laid: typeof geometry) => laid.pieces.find((piece) => piece.kind === "frame" && piece.look === "with")?.rect.width;
    expect(width(geometry)).toBe(width(layoutFlow(flow, { measure })));
  });

  it("draws place_order's pieces at their rects, with a wire per drawn route", () => {
    const { geometry, container } = show("01-place-order");
    const root = container.querySelector<HTMLElement>(".flow");
    expect(root?.style.width).toBe(`${geometry.width}px`);
    expect(all(container, ".flow-pill.start")).toHaveLength(1);
    expect(all(container, ".flow-pill.end")).toHaveLength(1);
    expect(all(container, ".flow-call.project").map((item) => item.dataset["key"])).toEqual([
      "shop/service.py#place_order/call:load_order",
      "shop/service.py#place_order/try[0]/body/call:charge_card",
      "shop/service.py#place_order/call:save_order",
      "shop/service.py#place_order/for[0]/body/call:reserve_stock",
    ]);
    expect(all(container, ".flow-frame.decision")).toHaveLength(1);
    expect(all(container, ".flow-frame.try")).toHaveLength(1);
    expect(all(container, ".flow-frame.loop")).toHaveLength(1);
    expect(all(container, ".flow-head.test").map((item) => item.textContent)).toEqual(["◆order is None", '◆order["paid"]']);
    expect(all(container, ".flow-section").map((item) => item.className)).toEqual(["flow-section try", "flow-section handler"]);
    expect(all(container, ".flow-exit.x-raise")[0]?.textContent).toBe("RaiseValueError→ raises to the caller");
    expect(all(container, ".flow-gap").map((item) => item.textContent)).toEqual(["⋯ 1 line", "⋯ 1 line"]);
    expect(all(container, ".flow-thin")[0]?.textContent).toContain("logger.warning");
    expect(all(container, ".flow-wire")).toHaveLength(geometry.wires.length);
    expect(all(container, ".flow-wire.dashed.error")).toHaveLength(1);
    const load = all(container, ".flow-call")[0];
    const rect = geometry.blocks.get("shop/service.py#place_order/call:load_order");
    expect(load?.style.left).toBe(`${rect?.x}px`);
    expect(load?.style.top).toBe(`${rect?.y}px`);
    expect(load?.querySelector(".flow-binds")?.textContent).toBe("→ order");
  });

  it("selects blocks and shows the selection", () => {
    const onSelect = vi.fn();
    const key = "shop/service.py#place_order/call:save_order";
    const { container } = show("01-place-order", { onSelect, selected: { kind: "block", key } });
    expect(all(container, ".selected").map((item) => item.dataset["key"])).toEqual([key]);
    fireEvent.click(screen.getByText("load_order"));
    expect(onSelect).toHaveBeenLastCalledWith({ kind: "block", key: "shop/service.py#place_order/call:load_order" });
    fireEvent.click(screen.getByText("order is None"));
    expect(onSelect).toHaveBeenLastCalledWith({ kind: "block", key: "shop/service.py#place_order/if[0]/then" });
    fireEvent.keyDown(screen.getByText("place_order"), { key: "Enter" });
    expect(onSelect).toHaveBeenLastCalledWith({ kind: "block", key: "shop/service.py#place_order" });
  });

  it("opens calls in place by their open path, and shows the ones that are open", () => {
    const onToggleOpen = vi.fn();
    const onSelect = vi.fn();
    const { container } = show("07-cli", { onToggleOpen, onSelect });
    expect(all(container, ".flow-inset")).toHaveLength(2);
    const open = all(container, '.flow-chevron[aria-expanded="true"]');
    expect(open.map((item) => item.getAttribute("aria-label"))).toEqual(["Close main", "Close place_order"]);
    fireEvent.click(screen.getByRole("button", { name: "Open load_order in place" }));
    expect(onToggleOpen).toHaveBeenCalledWith(["shop/cli.py#/if[0]/then/call:main", "shop/cli.py#main/call:place_order", "shop/service.py#place_order/call:load_order"]);
    expect(onSelect).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Open mark_failed in place" }));
    expect(onToggleOpen).toHaveBeenLastCalledWith(["shop/cli.py#/if[0]/then/call:main", "shop/cli.py#main/call:place_order", "shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed"]);
  });

  it("opens a callback in place from its row", () => {
    const onToggleOpen = vi.fn();
    const { container } = show("09-orders-route", { onToggleOpen });
    expect(all(container, ".flow-callback")[0]?.textContent).toContain("async (req, res) =>");
    fireEvent.click(screen.getByRole("button", { name: "Open callback to ordersRouter.post in place" }));
    expect(onToggleOpen).toHaveBeenCalledWith(['src/routes/orders.ts#/call:ordersRouter.post("/orders/:id")/callback[0]']);
  });

  it("lists and folds thin rows, and selects a listed call", () => {
    const onToggleRow = vi.fn();
    const onSelect = vi.fn();
    const gathered = show("08-api", { onToggleRow, onSelect });
    const row = all(gathered.container, ".flow-thin")[0];
    expect(row?.textContent).toContain("5 calls, 2 unknown");
    expect(all(gathered.container, ".flow-unknown")).toHaveLength(2);
    if (row) {
      fireEvent.click(row);
    }
    expect(onToggleRow).toHaveBeenCalledWith("shop/api.py#/call:FastAPI");
    gathered.container.remove();
    const listed = show("08-api", { onToggleRow, onSelect }, new Set(["shop/api.py#/call:FastAPI"]));
    expect(all(listed.container, ".flow-thin.listed")).toHaveLength(1);
    fireEvent.click(screen.getByText("Depends"));
    expect(onSelect).toHaveBeenCalledWith({ kind: "block", key: "shop/api.py#/call:Depends" });
  });

  it("draws combined steps with their counts and ways out, and toggles them", () => {
    const onToggleCombined = vi.fn();
    const onSelect = vi.fn();
    const { container } = show("place-order-shaped", { onToggleCombined, onSelect, selected: { kind: "combined", item: "stop" } });
    const combined = all(container, ".flow-combined.collapsed");
    expect(combined.map((item) => item.dataset["item"])).toEqual(["stop", "failed", "save"]);
    expect(combined[0]?.classList.contains("selected")).toBe(true);
    expect(combined[0]?.querySelector(".flow-combined-summary")?.textContent).toBe("2 conditions");
    expect([...(combined[0]?.querySelectorAll(".flow-combined-exit") ?? [])].map((item) => item.textContent)).toEqual(["raises→ the caller", "returns→ End"]);
    const toggles = screen.getAllByRole("button", { name: "open" });
    expect(toggles).toHaveLength(3);
    const [first] = toggles;
    if (first) {
      fireEvent.click(first);
    }
    expect(onToggleCombined).toHaveBeenCalledWith("stop", false);
    expect(onSelect).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("Save it and reserve stock"));
    expect(onSelect).toHaveBeenCalledWith({ kind: "combined", item: "save" });
  });

  it("opens a gap tick's lines in its file", () => {
    const onOpenLines = vi.fn();
    show("06-checkout", { onOpenLines });
    fireEvent.click(screen.getByRole("button", { name: "⋯ 3 lines · for loop" }));
    expect(onOpenLines).toHaveBeenCalledWith("shop/checkout.py", [15, 17]);
  });

  it("opens a thin row's lines from its `</>`, in the file of the flow drawing it", () => {
    const onViewRow = vi.fn();
    const onSelect = vi.fn();
    const { container } = show("08-api", { onViewRow, onSelect });
    const row = all(container, ".flow-thin")[0];
    const button = row?.querySelector<HTMLElement>(".flow-code");
    if (!button) {
      throw new Error("the row has a `</>`");
    }
    fireEvent.click(button);
    expect(onViewRow).toHaveBeenCalledWith("shop/api.py", expect.objectContaining({ lines: [6, 15] }));
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("marks AWAIT and SOMETIMES, and draws a sometimes call dashed", () => {
    const awaited = show("02-place-order-ts");
    expect(all(awaited.container, ".flow-tag").map((item) => item.textContent)).toEqual(["await", "await", "await"]);
    awaited.container.remove();
    const sometimes = show("05-route-message");
    expect(all(sometimes.container, ".flow-call.sometimes").map((item) => item.dataset["key"])).toEqual(["mail/route.py#route_message/call:fetch_profile"]);
    expect(all(sometimes.container, ".flow-tag.sometimes")).toHaveLength(1);
  });

  it("cuts a long test short with a visible …, and keeps it whole in its tooltip", () => {
    const test = 'not isinstance(value.get("email"), str) or "@" not in value["email"]';
    const flow = structuredClone(drawn("01-place-order"));
    const decision = flow.steps.steps.find((step) => step.kind === "decision");
    const [first, second] = decision?.kind === "decision" ? decision.lanes : [];
    if (!first?.test || !second?.test) {
      throw new Error("place_order's decision has two tests");
    }
    first.test.text = test;
    second.test.text = test;
    second.name = "Is the email missing?";
    const { container } = render(<FlowView geometry={layoutFlow(flow, { measure })} />);
    const [unnamed, named] = all(container, ".flow-head.test .flow-head-text");
    expect(unnamed?.textContent).toMatch(/^not isinstance\(.*…$/);
    expect(unnamed?.title).toBe(test);
    expect(named?.querySelector(".flow-name")?.textContent).toBe("Is the email missing?");
    expect(named?.querySelector(".flow-code-name")?.textContent).toMatch(/…$/);
    expect(named?.title).toBe(`Is the email missing?\n${test}`);
    // A test that fits has no tooltip.
    const whole = show("01-place-order");
    expect(all(whole.container, ".flow-head.test .flow-head-text").map((item) => [item.textContent, item.title])).toEqual([
      ["order is None", ""],
      ['order["paid"]', ""],
    ]);
  });
});

/** The outlines drawn over a flow: their kind and where they are. */
function outlines(container: HTMLElement): { readonly lit: boolean; readonly at: string }[] {
  return all(container, ".flow-outline").map((item) => ({ lit: item.classList.contains("lit"), at: `${item.style.left},${item.style.top},${item.style.width},${item.style.height}` }));
}

function at(rect: { readonly x: number; readonly y: number; readonly width: number; readonly height: number } | undefined): string {
  return rect ? `${rect.x}px,${rect.y}px,${rect.width}px,${rect.height}px` : "none";
}

describe("FlowView and the code panel", () => {
  it("outlines the selected block on its own shape", () => {
    const key = "shop/service.py#place_order/call:save_order";
    const { geometry, container } = show("01-place-order", { selected: { kind: "block", key } });
    expect(outlines(container)).toEqual([{ lit: false, at: at(geometry.blocks.get(key)) }]);
    expect(container.querySelector(".flow-outline")?.className).toBe("flow-outline box");
  });

  it("outlines what holds a block that isn't drawn on its own: a hidden control block's gap tick, a collapsed combined step", () => {
    const hidden = show("06-checkout", { selected: { kind: "block", key: "shop/checkout.py#checkout/for[0]" } });
    const tick = all(hidden.container, ".flow-gap")[0];
    expect(outlines(hidden.container)).toEqual([{ lit: false, at: `${tick?.style.left},${tick?.style.top},${tick?.style.width},${tick?.style.height}` }]);
    expect(hidden.container.querySelector(".flow-outline")?.className).toBe("flow-outline tick");
    hidden.container.remove();
    const combined = show("place-order-shaped", { selected: { kind: "block", key: "shop/service.py#place_order/call:save_order" } });
    expect(outlines(combined.container)).toEqual([{ lit: false, at: at(combined.geometry.items.get("save")) }]);
  });

  it("outlines the gap tick that counts a line, or a row of other calls, when nothing selected is drawn here", () => {
    const { geometry, container } = show("01-place-order", { selected: { kind: "block", key: "elsewhere" }, outline: { line: { path: "shop/service.py", line: 24 } } });
    const tick = geometry.pieces.find((piece) => piece.kind === "gap" && piece.gap.lines[0] === 24);
    expect(outlines(container)).toEqual([{ lit: false, at: at(tick?.rect) }]);
    container.remove();
    const row = show("01-place-order", { outline: { block: "shop/service.py#place_order/try[0]/except[0]/call:logger.warning" } });
    const thin = row.geometry.pieces.find((piece) => piece.kind === "thin");
    expect(outlines(row.container)).toEqual([{ lit: false, at: at(thin?.rect) }]);
  });

  it("lights a hovered line's block, or the gap tick of a line no block covers", () => {
    const key = "shop/service.py#place_order/call:load_order";
    const block = show("01-place-order", { hover: { key, block: key, path: "shop/service.py", line: 11 } });
    expect(outlines(block.container)).toEqual([{ lit: true, at: at(block.geometry.blocks.get(key)) }]);
    block.container.remove();
    // A builtin isn't drawn on its own: the line's block lights instead.
    const save = "shop/service.py#place_order/call:save_order";
    const builtin = show("01-place-order", { hover: { key: "shop/service.py#place_order/call:len", block: save, path: "shop/service.py", line: 25 } });
    expect(outlines(builtin.container)).toEqual([{ lit: true, at: at(builtin.geometry.blocks.get(save)) }]);
    builtin.container.remove();
    const gap = show("01-place-order", { hover: { key: null, block: null, path: "shop/service.py", line: 17 } });
    const tick = gap.geometry.pieces.find((piece) => piece.kind === "gap" && piece.gap.lines[0] === 17);
    expect(outlines(gap.container)).toEqual([{ lit: true, at: at(tick?.rect) }]);
    gap.container.remove();
    const elsewhere = show("01-place-order", { hover: { key: null, block: null, path: "shop/other.py", line: 17 } });
    expect(outlines(elsewhere.container)).toEqual([]);
  });

  it("opens a block's code from its `</>` without selecting it twice", () => {
    const onViewCode = vi.fn();
    const onSelect = vi.fn();
    const { container } = show("place-order-shaped", { onViewCode, onSelect });
    const call = container.querySelector('.flow-call[data-key="shop/service.py#place_order/call:load_order"]');
    const button = call?.querySelector<HTMLElement>(":scope > .flow-code");
    expect(button?.textContent).toBe("</>");
    if (button) {
      fireEvent.click(button);
    }
    expect(onViewCode).toHaveBeenCalledWith({ kind: "block", key: "shop/service.py#place_order/call:load_order" });
    expect(onSelect).not.toHaveBeenCalled();
    const combined = container.querySelector<HTMLElement>('.flow-combined[data-item="save"] > .flow-code');
    if (combined) {
      fireEvent.click(combined);
    }
    expect(onViewCode).toHaveBeenLastCalledWith({ kind: "combined", item: "save" });
    // Every block has one: frames, parts, exits, rows of other calls, and combined steps.
    expect(all(container, ".flow-code").length).toBeGreaterThan(5);
    expect(container.querySelector(".flow-pill.start .flow-code")).toBeNull();
  });

  it("has no `</>` until something opens code", () => {
    const { container } = show("01-place-order");
    expect(all(container, ".flow-code")).toHaveLength(0);
  });

  it("opens a block's menu on a right-click", () => {
    const onBlockMenu = vi.fn();
    show("01-place-order", { onBlockMenu });
    fireEvent.contextMenu(screen.getByText("load_order"));
    expect(onBlockMenu).toHaveBeenCalledWith(expect.anything(), { kind: "block", key: "shop/service.py#place_order/call:load_order" });
  });
});
