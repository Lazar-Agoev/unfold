import { describe, expect, it } from "vitest";
import type { DrawnFlow, Lane, Route } from "../ipc/generated";
import { combinedExits, exitDestination, sectionWords } from "./words";

const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

/** The first part of `form` anywhere in a drawn golden, as the IPC sends it. */
function laneOf(name: string, form: Lane["form"]): Lane {
  const pending: unknown[] = [GOLDENS[`../../../testdata/goldens/drawn/${name}.json`]];
  while (pending.length > 0) {
    const value = pending.pop();
    if (value === null || typeof value !== "object") {
      continue;
    }
    const record = value as Record<string, unknown>;
    if (record["form"] === form && "lines" in record && "steps" in record) {
      return structuredClone(record) as Lane;
    }
    pending.push(...Object.values(record));
  }
  throw new Error(`no ${form} part in ${name}`);
}

function withClause(lane: Lane, text: string): Lane {
  return { ...lane, test: { text, calls: lane.test?.calls ?? [], unknown: lane.test?.unknown ?? [] } };
}

describe("sectionWords", () => {
  it("titles a handler with its keyword and keeps the clause", () => {
    const handler = laneOf("01-place-order", "except");
    expect(sectionWords(handler, false)).toEqual({ title: "Except", clause: "PaymentDeclined as error", note: null });
  });

  it("titles an except* handler with its star, however it is spaced", () => {
    const handler = { ...laneOf("01-place-order", "except"), group: true };
    for (const text of ["except* PaymentDeclined as error", "except *PaymentDeclined as error"]) {
      expect(sectionWords(withClause(handler, text), false)).toEqual({ title: "Except*", clause: "PaymentDeclined as error", note: null });
    }
  });

  it("knows an except* handler by its lane, never by its clause's text", () => {
    const handler = laneOf("01-place-order", "except");
    expect(handler.group).toBe(false);
    expect(sectionWords(withClause(handler, "except *PaymentDeclined as error"), false).title).toBe("Except");
  });
});

describe("where a transfer lands", () => {
  const handler = { ...withClause(laneOf("01-place-order", "except"), "except ValueError"), id: 7 };
  const cleanup = { ...laneOf("03-sync-inbox", "finally"), id: 9 };
  const lanes = new Map<number, Lane>([
    [handler.id, handler],
    [cleanup.id, cleanup],
  ]);
  const route = (kind: Route["kind"], to: Route["to"], through: number[] = []): Route => ({ from: 3, kind, to, through });

  it("names the handler that catches a raise", () => {
    expect(exitDestination([route("raise", { step: 7 })], "raise", false, lanes)).toBe("except ValueError");
    expect(exitDestination([route("raise", "caller", [9])], "raise", false, lanes)).toBe("finally, then raises to the caller");
  });

  it("ends a transfer a finally always leaves in that finally", () => {
    expect(exitDestination([route("break", { step: 9 })], "break", false, lanes)).toBe("finally");
    expect(combinedExits([route("continue", { step: 9 }), route("return", "end")], false, lanes)).toEqual([
      { kind: "continue", verb: "continues", destination: "finally" },
      { kind: "return", verb: "returns", destination: "End" },
    ]);
  });
});
