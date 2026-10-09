// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { applyTheme, parseThreadLimit } from "./PreferencesDialog";

describe("preferences", () => {
  it("parses the thread limit: empty is automatic, otherwise 1 to 256", () => {
    expect(parseThreadLimit("")).toEqual({ ok: true, value: null });
    expect(parseThreadLimit(" 8 ")).toEqual({ ok: true, value: 8 });
    expect(parseThreadLimit("0").ok).toBe(false);
    expect(parseThreadLimit("2.5").ok).toBe(false);
    expect(parseThreadLimit("lots").ok).toBe(false);
  });

  it("applies the theme through data-theme, and follows the OS for system", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset["theme"]).toBe("dark");
    applyTheme("system");
    expect(document.documentElement.dataset["theme"]).toBeUndefined();
  });
});
