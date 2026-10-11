import { describe, expect, it } from "vitest";
import { methodColumn } from "./entrances";

describe("methodColumn", () => {
  it("uses the HTTP method, or the kind in capitals", () => {
    expect(methodColumn({ kind: "http", method: "DELETE" })).toBe("DELETE");
    expect(methodColumn({ kind: "command", method: null })).toBe("COMMAND");
  });
});
