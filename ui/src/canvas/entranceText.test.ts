import { describe, expect, it } from "vitest";
import type { TargetView } from "../ipc/generated";
import { targetsHeading } from "./entranceText";

function target(position: number, role: TargetView["role"]): TargetView {
  return { position, role, text: `t${position}`, resolution: "resolved", target: `src/a.ts#function:t${position}`, name: `t${position}`, path: "src/a.ts", line: 1, note: null, reason: null, module: null };
}

describe("an expanded entrance's heading", () => {
  it("follows the roles its targets were returned with", () => {
    expect(targetsHeading([target(0, "startup")])).toBe("Starts");
    expect(targetsHeading([target(0, "handler"), target(1, "handler")])).toBe("Handlers, in order");
    expect(targetsHeading([target(0, "startup"), target(1, "handler")])).toBe("Handlers, in order");
    expect(targetsHeading([])).toBe("Handlers, in order");
  });
});
