// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { StartTarget } from "../../../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../../../test/engine";
import { AgentData } from "./data";

const P = "p-orders0000";
const START = "entrance:shop-api/place-an-order";

let engine: FakeEngine;

/** The engine draws `START` as a start card whose code is `code`. */
function install(code: readonly StartTarget[]): void {
  engine = installFakeEngine({
    handlers: {
      card: () => ({ type: "start", key: START, kind: "entrance", app: "Shop API", name: "Place an order", code: [...code] }),
    },
  });
}

beforeEach(() => {
  queryClient.clear();
});

afterEach(() => {
  engine.uninstall();
});

describe("following from a start", () => {
  it("goes through code that is found or stale, as tracing by hand does", async () => {
    install([
      { key: "shop/api.py#load_order", state: "found" },
      { key: "shop/api.py#create_order", state: "stale" },
    ]);
    const calls = await new AgentData().followableCalls(P, START);
    expect(calls).toEqual({ resolved: [{ target: "shop/api.py#load_order" }, { target: "shop/api.py#create_order" }], blocked: [] });
  });

  it("stops at code that is missing, not analyzed, or not a start, and says where", async () => {
    install([
      { key: "shop/api.py#gone", state: "missing" },
      { key: "legacy/export.py#run", state: "not_analyzed" },
      { key: "shop/api.py#create_order/call:charge", state: "not_a_start" },
    ]);
    const calls = await new AgentData().followableCalls(P, START);
    expect(calls).toEqual({
      resolved: [],
      blocked: [
        { reason: "unresolved", call: "shop/api.py#gone", note: "no block has this key" },
        { reason: "unresolved", call: "legacy/export.py#run", note: "its file isn't analyzed" },
        { reason: "unresolved", call: "shop/api.py#create_order/call:charge", note: "not a start" },
      ],
    });
  });
});
