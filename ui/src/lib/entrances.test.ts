import { describe, expect, it } from "vitest";
import { methodColumn, triggerText } from "./entrances";

describe("methodColumn", () => {
  it("uses the HTTP method, or the kind in capitals", () => {
    expect(methodColumn({ kind: "http", method: "DELETE" })).toBe("DELETE");
    expect(methodColumn({ kind: "command", method: null })).toBe("COMMAND");
  });
});

describe("triggerText", () => {
  const row = { kind: "http", label: null, trigger: "/orders", triggerStatus: "exact" } as const;

  it("prefers a label, then the trigger", () => {
    expect(triggerText({ ...row, label: "Checkout" })).toBe("Checkout");
    expect(triggerText(row)).toBe("/orders");
  });

  it("marks a partial trigger and says when it is unknown", () => {
    expect(triggerText({ ...row, triggerStatus: "partial" })).toBe("/orders…");
    expect(triggerText({ ...row, trigger: null, triggerStatus: "unknown" })).toBe("path unknown");
    expect(triggerText({ ...row, kind: "worker", trigger: null, triggerStatus: "unknown" })).toBe("trigger unknown");
  });
});
