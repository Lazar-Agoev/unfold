// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { copyDiagnostics, diagnosticsText, shellVersions } from "./diagnostics";
import { problemText, settingsProblem } from "./edits";

let engine: FakeEngine | null = null;

afterEach(() => {
  engine?.uninstall();
  engine = null;
});

describe("settings problems", () => {
  it("splits the position out of Rust's message", () => {
    const problem = settingsProblem("invalid_settings", "unfold.toml:31:11: `src/a.ts#a.b.c` is not a valid locator");
    expect(problem).toEqual({ code: "invalid_settings", message: "`src/a.ts#a.b.c` is not a valid locator", line: 31, column: 11 });
    expect(problemText(problem)).toBe("Line 31, column 11: `src/a.ts#a.b.c` is not a valid locator");
    const prefixed = settingsProblem("invalid_settings", "fix unfold.toml first: unfold.toml:2:1: unknown field `bogus`");
    expect(prefixed.line).toBe(2);
    expect(prefixed.message).toBe("fix unfold.toml first: unknown field `bogus`");
    expect(settingsProblem("settings_conflict", "no `[[entrance]]` has the id `gone`")).toMatchObject({ line: null, column: null });
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

describe("Copy Diagnostics", () => {
  it("copies Rust's summary with the shell's versions through the main process", async () => {
    engine = installFakeEngine({ handlers: { diagnostics: () => ({ text: "Unfold diagnostics\nEngine version: 0.1.0\n" }) } });
    await copyDiagnostics();
    expect(engine.windowCalls).toHaveLength(1);
    const call = engine.windowCalls[0];
    expect(call?.command).toBe("copy_to_clipboard");
    const text = (call?.args as { text: string }).text;
    expect(text.startsWith("Unfold diagnostics\nApp: ")).toBe(true);
    expect(text).toContain("Engine version: 0.1.0");
  });
});
