import { describe, expect, it, vi } from "vitest";
import type { CallContract } from "../engine-types";
import { readCall } from "./calls";
import { harness, PROJECT, refusal } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const CALL = "orders/create.ts#createOrder/try[0]/body/call:chargePayment";
const BOARD = { project: PROJECT, id: ".unfold/boards/typed.json" };

/** The typed-lines fixture's call from createOrder to chargePayment (design §8.7). */
const CONTRACT: CallContract = {
  call: CALL,
  line: 27,
  text: "chargePayment",
  callee: { outcome: "project", key: "payments.ts#chargePayment" },
  sends: [
    { argument: { index: 0, text: "cart" }, parameter: { index: 0, name: "cart", kind: "positional", optional: false, default: false, type: { text: "Cart", spans: [{ start: 0, end: 4, to: { outcome: "project", key: "cart.ts#Cart" } }] } } },
    { argument: { index: 1, text: "req.user" }, parameter: { index: 1, name: "user", kind: "positional", optional: false, default: false, type: { text: "User", spans: [{ start: 0, end: 4, to: { outcome: "project", key: "user.ts#User" } }] } } },
    { argument: { index: 2, text: "options", expand: "spread" }, unpaired: { reason: "spread", note: "a spread argument fills no proven parameter" } },
    { parameter: { index: 2, name: "retries", kind: "positional", optional: true, default: true } },
  ],
  returns: { awaited: true, type: { text: "Promise<Charge>", spans: [{ start: 8, end: 14, to: { outcome: "project", key: "payments.ts#Charge" } }] }, lands: { kind: "binds", target: "charge" } },
  raises: [
    {
      exit: "payments.ts#chargePayment/if[0]/then/throw[0]",
      line: 17,
      type: { text: "CardDeclinedError", spans: [{ start: 0, end: 17, to: { outcome: "project", key: "payments.ts#CardDeclinedError" } }] },
      caughtBy: "orders/create.ts#createOrder/try[0]/catch[0]",
    },
  ],
};

describe("read_call", () => {
  it("reads a call's contract by its key", async () => {
    const app = harness({ engine: { call_contract: () => CONTRACT } });
    const { text } = await app.call(readCall, { call: CALL, project: "ts-orders" });
    expect(app.engineCalls.at(-1)).toEqual(["call_contract", { project: PROJECT, key: CALL }]);
    expect(text.split("\n").slice(1)).toEqual([
      `The call chargePayment at line 27 (key ${CALL}) → chargePayment (key payments.ts#chargePayment).`,
      "Sends (4):",
      "- cart → cart: Cart (Cart: cart.ts#Cart)",
      "- req.user → user: User (User: user.ts#User)",
      "- ...options → no proven parameter: a spread argument fills no proven parameter",
      "- (no argument) → retries? (no written type), has a default",
      "Returns Promise<Charge> (Charge: payments.ts#Charge), awaited; the value lands in charge.",
      "Raises directly (1): CardDeclinedError (CardDeclinedError: payments.ts#CardDeclinedError) at line 17 (key payments.ts#chargePayment/if[0]/then/throw[0]), caught by orders/create.ts#createOrder/try[0]/catch[0].",
      "Next: read_code on the callee for its code; written types only, so say in chat what isn't written.",
    ]);
  });

  it("reads each call of the arrow selected in the person's view", async () => {
    const app = harness({ engine: { call_contract: (args) => ({ ...CONTRACT, call: String(args["key"]) }) } });
    app.report({ board: BOARD, page: "page-1", selection: { kind: "arrow", from: "a", to: "b", calls: [CALL] }, visible: [], held: [BOARD] });
    const one = await app.call(readCall, {});
    expect(one.text).toContain(`(key ${CALL})`);
    expect(one.text).not.toContain("The selected arrow draws");
    app.report({ board: BOARD, page: "page-1", selection: { kind: "arrow", from: "a", to: "b", calls: [CALL, `${CALL}x`] }, visible: [], held: [BOARD] });
    const merged = await app.call(readCall, {});
    expect(merged.text).toContain("The selected arrow draws 2 calls:");
    expect(merged.text).toContain(`(key ${CALL}x)`);
  });

  it("teaches how to name a call", async () => {
    const app = harness({
      engine: {
        call_contract: () => {
          throw refusal("not_found", "no call at that key");
        },
      },
    });
    expect((await app.call(readCall, { call: CALL })).text).toBe(`Can't read the contract: ${CALL} isn't a call Unfold compiled. read_code lists each call with its key.`);
    expect((await app.call(readCall, {})).text).toBe(
      "Can't tell which call you mean: no call was given and no arrow is selected in Unfold. Pass call (a call's key from read_code), or ask the person to select the arrow.",
    );
  });
});
