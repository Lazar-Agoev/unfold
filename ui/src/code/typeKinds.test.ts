import { describe, expect, it } from "vitest";
import { memberName } from "./typeKinds";

describe("a type's members", () => {
  it("are named as written, keeping `?` and naming signatures", () => {
    expect(memberName({ name: "coupon", kind: "property", optional: true, readonly: true, default: false })).toBe("coupon?");
    expect(memberName({ kind: "call", optional: false, readonly: false, default: false })).toBe("()");
    expect(memberName({ kind: "construct", optional: false, readonly: false, default: false })).toBe("new ()");
  });
});
