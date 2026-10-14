// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DrawnFlow, Key } from "../ipc/generated";
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

describe("FlowView", () => {
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
    const folded = show("08-api", { onToggleRow, onSelect });
    const row = all(folded.container, ".flow-thin")[0];
    expect(row?.textContent).toContain("5 calls, 2 unknown");
    expect(all(folded.container, ".flow-unknown")).toHaveLength(2);
    if (row) {
      fireEvent.click(row);
    }
    expect(onToggleRow).toHaveBeenCalledWith("shop/api.py#/call:FastAPI");
    folded.container.remove();
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

  it("marks AWAIT and SOMETIMES, and draws a sometimes call dashed", () => {
    const awaited = show("02-place-order-ts");
    expect(all(awaited.container, ".flow-tag").map((item) => item.textContent)).toEqual(["await", "await", "await"]);
    awaited.container.remove();
    const sometimes = show("05-route-message");
    expect(all(sometimes.container, ".flow-call.sometimes").map((item) => item.dataset["key"])).toEqual(["mail/route.py#route_message/call:fetch_profile"]);
    expect(all(sometimes.container, ".flow-tag.sometimes")).toHaveLength(1);
  });
});
