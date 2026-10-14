import { describe, expect, it } from "vitest";
import type { DrawnFlow, Lane } from "../ipc/generated";
import { sectionWords } from "./words";

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
  return { ...lane, test: { text, calls: lane.test?.calls ?? [] } };
}

describe("sectionWords", () => {
  it("titles a handler with its keyword and keeps the clause", () => {
    const handler = laneOf("01-place-order", "except");
    expect(sectionWords(handler, false)).toEqual({ title: "Except", clause: "PaymentDeclined as error", note: null });
  });

  it("titles an except* handler with its star, however it is spaced", () => {
    const handler = laneOf("01-place-order", "except");
    for (const text of ["except* PaymentDeclined as error", "except *PaymentDeclined as error"]) {
      expect(sectionWords(withClause(handler, text), false)).toEqual({ title: "Except*", clause: "PaymentDeclined as error", note: null });
    }
  });
});
