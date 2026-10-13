import { describe, expect, it, vi } from "vitest";
import type { CallContract } from "../engine-types";
import { readCall } from "./calls";
import { harness, PROJECT, refusal } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const SOURCE = "orders/create.ts#function:createOrder/variable:charge/call[0]";
const BOARD = { project: PROJECT, id: ".unfold/boards/typed.json" };

/** The typed-lines fixture's call from createOrder to chargePayment (design §8.7). */
const CONTRACT: CallContract = {
  source: SOURCE,
  seq: 0,
  target: "payments.ts#function:chargePayment",
  callerLine: 27,
  sends: [
    { argument: { index: 0, text: "cart" }, parameter: { index: 0, name: "cart", kind: "positional", optional: false, default: false, type: { text: "Cart", spans: [{ start: 0, end: 4, style: "declared", target: "cart.ts#interface:Cart" }] } } },
    { argument: { index: 1, text: "req.user" }, parameter: { index: 1, name: "user", kind: "positional", optional: false, default: false, type: { text: "User", spans: [{ start: 0, end: 4, style: "declared", target: "user.ts#interface:User" }] } } },
    { argument: { index: 2, text: "options", expand: "spread" }, unpaired: { reason: "spread", note: "a spread argument fills no proven parameter" } },
    { parameter: { index: 2, name: "retries", kind: "positional", optional: true, default: true } },
  ],
  returns: { binding: "variable", text: "charge", awaited: true, type: { text: "Promise<Charge>", spans: [{ start: 8, end: 14, style: "declared", target: "payments.ts#interface:Charge" }] } },
  throws: [{ type: { text: "CardDeclinedError", spans: [{ start: 0, end: 17, style: "declared", target: "payments.ts#class:CardDeclinedError" }] }, line: 17 }],
  caught: true,
  calleeLines: [12, 17],
};

describe("read_call", () => {
  it("reads a call's contract by its source and seq", async () => {
    const app = harness({ engine: { call_contract: () => CONTRACT } });
    const { text } = await app.call(readCall, { source: SOURCE, project: "ts-orders" });
    expect(app.engineCalls.at(-1)).toEqual(["call_contract", { project: PROJECT, source: SOURCE, seq: 0 }]);
    expect(text.split("\n").slice(1)).toEqual([
      `The call at orders/create.ts:27 (source ${SOURCE}, seq 0) → chargePayment (key payments.ts#function:chargePayment).`,
      "Sends (4):",
      "- cart → cart: Cart (Cart: cart.ts#interface:Cart)",
      "- req.user → user: User (User: user.ts#interface:User)",
      "- ...options → no proven parameter: a spread argument fills no proven parameter",
      "- (no argument) → retries? (no written type), has a default",
      "Returns Promise<Charge> (Charge: payments.ts#interface:Charge), awaited; the value lands in the variable charge.",
      "Throws directly (1): CardDeclinedError (CardDeclinedError: payments.ts#class:CardDeclinedError) at payments.ts:17. The call sits in a try with a catch in the caller, so the caller catches them.",
      "Read from payments.ts lines 12, 17.",
      "Next: read_code on the callee for its source; written types only, so say in chat what isn't written.",
    ]);
  });

  it("reads the line selected in the person's view, and each call of a merged arrow", async () => {
    const app = harness({ engine: { call_contract: (args) => ({ ...CONTRACT, source: String(args["source"]) }) } });
    app.report({ board: BOARD, page: "page-1", selection: { kind: "line", from: "a", to: "b", source: SOURCE, seq: 0, line: 27 }, visible: [], held: [BOARD] });
    expect((await app.call(readCall, {})).text).toContain(`(source ${SOURCE}, seq 0)`);
    app.report({
      board: BOARD,
      page: "page-1",
      selection: { kind: "calls", from: "a", to: "b", calls: [{ source: SOURCE, seq: 0, line: 27 }, { source: `${SOURCE}x`, seq: 0, line: 30 }] },
      visible: [],
      held: [BOARD],
    });
    const merged = await app.call(readCall, {});
    expect(merged.text).toContain("The selected arrow draws 2 calls:");
    expect(merged.text).toContain(`(source ${SOURCE}x, seq 0)`);
  });

  it("teaches which calls have contracts, and how to name one", async () => {
    const app = harness({
      engine: {
        call_contract: () => {
          throw refusal("not_found", "no resolved call at that occurrence");
        },
      },
    });
    expect((await app.call(readCall, { source: SOURCE })).text).toBe(
      `Can't read the contract: ${SOURCE} (seq 0) isn't a resolved call or construction. Only those have contracts; read_code lists each call with where it goes and its source and seq.`,
    );
    expect((await app.call(readCall, {})).text).toBe(
      "Can't tell which call you mean: no source was given and no line is selected in Unfold. Pass source (an occurrence key from read_code's calls) and seq, or ask the person to select the line.",
    );
  });
});
