import { describe, expect, it } from "vitest";
import type { ManualEntranceView } from "../ipc/generated";
import { diagnosticsText, shellVersions } from "./diagnostics";
import { draftOf, emptyDraft, hasProblems, manualTableOf, specOf, validateDraft, type EntranceDraft } from "./entrance";
import { canBeHandler, locatorFor, locatorLanguage, locatorProblem } from "./locators";
import { problemText, settingsProblem } from "./store";

function table(index: number, kind: ManualEntranceView["kind"], trigger: string | null, handler: string[], extra: Partial<ManualEntranceView> = {}): ManualEntranceView {
  return { index, kind, trigger, method: null, label: null, handler, startup: null, line: 10 + index * 4, ...extra };
}

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

describe("entrance drafts", () => {
  const command: EntranceDraft = { ...emptyDraft("src/cli.ts#report"), trigger: "report" };

  it("accepts a well-formed command and leaves blanks out of the spec", () => {
    expect(hasProblems(validateDraft(command))).toBe(false);
    expect(specOf({ ...command, label: "  ", method: "POST" })).toEqual({
      kind: "command",
      trigger: "report",
      method: null,
      label: null,
      handler: ["src/cli.ts#report"],
      startup: null,
    });
  });

  it("keeps the method for HTTP only", () => {
    const http: EntranceDraft = { ...command, kind: "http", trigger: "/orders", method: "POST", handlers: ["src/auth.ts#requireUser", "", "src/routes/orders.ts#createOrder"] };
    expect(specOf(http)).toMatchObject({ kind: "http", method: "POST", handler: ["src/auth.ts#requireUser", "src/routes/orders.ts#createOrder"] });
    expect(specOf({ ...http, method: "" }).method).toBeNull();
  });

  it("needs a startup for a process, and no handlers", () => {
    const process: EntranceDraft = { ...command, kind: "process", trigger: "" };
    expect(validateDraft(process).startup).toMatch(/starts from/);
    expect(validateDraft({ ...process, startup: "src/server.ts#" }).startup).toMatch(/name/);
    const ready = { ...process, startup: " src/server.ts " };
    expect(hasProblems(validateDraft(ready))).toBe(false);
    expect(specOf(ready)).toMatchObject({ kind: "process", trigger: null, handler: [], startup: "src/server.ts" });
  });

  it("flags blank triggers, missing or bad handlers, and mixed languages", () => {
    expect(validateDraft({ ...command, trigger: "   " }).trigger).toMatch(/blank/);
    expect(validateDraft({ ...command, trigger: "" }).trigger).toBeUndefined();
    expect(validateDraft({ ...command, handlers: ["", " "] }).handlerList).toMatch(/at least one/);
    expect(validateDraft({ ...command, handlers: ["src/a.ts#x", "src/b.ts:0"] }).handlers).toEqual([null, expect.stringMatching(/line/)]);
    expect(validateDraft({ ...command, handlers: ["src/a.ts#x", "app/b.py#y"] }).handlerList).toMatch(/mix/);
  });

  it("starts from an existing table", () => {
    expect(draftOf(table(2, "process", null, [], { startup: "src/server.ts" }))).toEqual({
      kind: "process",
      trigger: "",
      method: "",
      label: "",
      handlers: [""],
      startup: "src/server.ts",
    });
  });
});

describe("manual entrance rows", () => {
  const tables = [
    table(0, "command", "import", ["src/jobs/import.ts#importOrders"]),
    table(1, "command", "retry", ["src/jobs/retry.ts#retry"]),
    table(2, "command", "retry", ["src/jobs/retry.ts#retry"]),
    table(3, "http", "/x", ["src/a.ts:12"], { method: "GET" }),
    table(4, "process", null, [], { startup: "src/server.ts" }),
  ];

  it("finds the table a row came from, counting repeats", () => {
    const key = (parts: string[]) => `entrance:${JSON.stringify(parts)}`;
    expect(manualTableOf({ key: key(["manual", "command", "", "import", "src/jobs/import.ts#importOrders"]), detector: "manual" }, tables)?.index).toBe(0);
    expect(manualTableOf({ key: key(["manual", "command", "", "retry", "src/jobs/retry.ts#retry"]), detector: "manual" }, tables)?.index).toBe(1);
    expect(manualTableOf({ key: `${key(["manual", "command", "", "retry", "src/jobs/retry.ts#retry"])}#1`, detector: "manual" }, tables)?.index).toBe(2);
    expect(manualTableOf({ key: key(["manual", "http", "GET", "/x", "src/a.ts:12"]), detector: "manual" }, tables)?.index).toBe(3);
    expect(manualTableOf({ key: key(["manual", "process", "", "", "src/server.ts"]), detector: "manual" }, tables)?.index).toBe(4);
  });

  it("ignores detected entrances and stale rows", () => {
    expect(manualTableOf({ key: 'entrance:["express.route","http","GET","/x","src/a.ts:12"]', detector: "express.route" }, tables)).toBeNull();
    expect(manualTableOf({ key: 'entrance:["manual","command","","gone","src/x.ts#y"]', detector: "manual" }, tables)).toBeNull();
    expect(manualTableOf({ key: "entrance:not json", detector: "manual" }, tables)).toBeNull();
  });
});

describe("settings problems", () => {
  it("splits the position out of Rust's message", () => {
    const problem = settingsProblem("invalid_settings", ".unfold/config.toml:31:11: `src/a.ts#a.b.c` is not a valid locator");
    expect(problem).toEqual({ code: "invalid_settings", message: "`src/a.ts#a.b.c` is not a valid locator", line: 31, column: 11 });
    expect(problemText(problem)).toBe("Line 31, column 11: `src/a.ts#a.b.c` is not a valid locator");
    const prefixed = settingsProblem("invalid_settings", "fix .unfold/config.toml first: .unfold/config.toml:2:1: unknown field `bogus`");
    expect(prefixed.line).toBe(2);
    expect(prefixed.message).toBe("fix .unfold/config.toml first: unknown field `bogus`");
    expect(settingsProblem("settings_conflict", "there is no manual entrance at position 9")).toMatchObject({ line: null, column: null });
  });
});

describe("diagnostics", () => {
  const userAgent =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Unfold/0.1.0 Chrome/146.0.7680.65 Electron/44.7.0 Safari/537.36";

  it("reads the shell's versions from the user agent", () => {
    expect(shellVersions(userAgent)).toBe("App: Unfold 0.1.0\nElectron: 44.7.0\nChromium: 146.0.7680.65");
    expect(shellVersions("Mozilla/5.0")).toBe("App: unknown\nElectron: unknown\nChromium: unknown");
  });

  it("puts the shell's versions under the title", () => {
    expect(diagnosticsText("Unfold diagnostics\nEngine version: 1\n", userAgent).split("\n").slice(0, 5)).toEqual([
      "Unfold diagnostics",
      "App: Unfold 0.1.0",
      "Electron: 44.7.0",
      "Chromium: 146.0.7680.65",
      "Engine version: 1",
    ]);
  });
});
