import { describe, expect, it } from "vitest";
import { canBeHandler, locatorFor, locatorLanguage, locatorProblem } from "./locators";

describe("locators", () => {
  it("accepts every form of the grammar", () => {
    for (const locator of ["src/server.ts", "src/cli.ts#runReport", "src/a.ts#OrderService.place", "src/a.ts#default", "src/cli.ts:12", "src/ünï/ä.ts#$init_2"]) {
      expect(locatorProblem(locator), locator).toBeNull();
    }
  });

  it("explains malformed locators", () => {
    expect(locatorProblem("")).toMatch(/folder-relative/);
    expect(locatorProblem("/abs/a.ts")).toMatch(/relative/);
    expect(locatorProblem("C:/a.ts")).toMatch(/relative/);
    expect(locatorProblem("src\\a.ts")).toMatch(/\//);
    expect(locatorProblem("src/../a.ts")).toMatch(/“\.\.”/);
    expect(locatorProblem("src//a.ts")).toMatch(/empty/);
    expect(locatorProblem("src/a.ts#a#b")).toMatch(/at most one/);
    expect(locatorProblem("src/a.ts#a:3")).toMatch(/at most one/);
    expect(locatorProblem("src/a.ts#a.b.c")).toMatch(/Owner\.member/);
    expect(locatorProblem("src/a.ts#1abc")).toMatch(/name/);
    expect(locatorProblem("src/a.ts#")).toMatch(/name/);
    expect(locatorProblem("src/a.ts:0")).toMatch(/line number/);
    expect(locatorProblem("src/a.ts:012")).toMatch(/line number/);
    expect(locatorProblem("src/a.ts:+3")).toMatch(/line number/);
  });

  it("tells TypeScript from Python by extension", () => {
    expect(locatorLanguage("src/a.tsx#App")).toBe("typescript");
    expect(locatorLanguage("src/app/main.py:3")).toBe("python");
    expect(locatorLanguage("src/a.js")).toBeNull();
  });

  it("points at definitions by name where a name can select them", () => {
    expect(locatorFor({ path: "src/routes/orders.ts", kind: "function", name: "createOrder", qualifiedName: "createOrder", line: 25 })).toBe(
      "src/routes/orders.ts#createOrder",
    );
    expect(locatorFor({ path: "src/svc.ts", kind: "method", name: "place", qualifiedName: "OrderService.place", line: 13 })).toBe(
      "src/svc.ts#OrderService.place",
    );
    // Nested deeper than Owner.member: by line.
    expect(locatorFor({ path: "src/a.ts", kind: "function", name: "inner", qualifiedName: "outer.inner", line: 7 })).toBe("src/a.ts:7");
    expect(canBeHandler("method")).toBe(true);
    expect(canBeHandler("if")).toBe(false);
  });
});
