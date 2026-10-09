import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { APP_CONTENT_SECURITY_POLICY, contentType, isUiUrl, resolveAppFile } from "./app-url";

const ui = resolve("/app/ui");

describe("resolveAppFile", () => {
  it("serves files inside the UI directory, and index.html for /", () => {
    expect(resolveAppFile(ui, "app://unfold/")).toBe(join(ui, "index.html"));
    expect(resolveAppFile(ui, "app://unfold/assets/index-abc.js")).toBe(join(ui, "assets", "index-abc.js"));
    expect(resolveAppFile(ui, "app://unfold/assets/a%20b.css?v=1#x")).toBe(join(ui, "assets", "a b.css"));
  });

  it("refuses other hosts, other schemes, and paths that escape", () => {
    expect(resolveAppFile(ui, "app://evil/index.html")).toBeNull();
    expect(resolveAppFile(ui, "file:///app/ui/index.html")).toBeNull();
    expect(resolveAppFile(ui, "app://unfold/..%2F..%2Fetc%2Fpasswd")).toBeNull();
    // The URL parser treats an encoded ".." as a dot segment, so it stays inside the root.
    expect(resolveAppFile(ui, "app://unfold/%2e%2e/%2e%2e/secret")).toBe(join(ui, "secret"));
    expect(resolveAppFile(ui, "app://unfold/bad%00name")).toBeNull();
    expect(resolveAppFile(ui, "app://unfold/%E0%A4%A")).toBeNull();
  });
});

describe("isUiUrl", () => {
  it("trusts only the app origin in production", () => {
    const source = { kind: "app", dir: ui } as const;
    expect(isUiUrl(source, "app://unfold/index.html")).toBe(true);
    expect(isUiUrl(source, "app://other/index.html")).toBe(false);
    expect(isUiUrl(source, "https://unfold/index.html")).toBe(false);
    expect(isUiUrl(source, "not a url")).toBe(false);
  });

  it("trusts only the dev server origin in development", () => {
    const source = { kind: "dev", url: "http://localhost:5173/" } as const;
    expect(isUiUrl(source, "http://localhost:5173/src/main.tsx")).toBe(true);
    expect(isUiUrl(source, "http://localhost:5174/")).toBe(false);
  });
});

describe("policy and types", () => {
  it("allows no inline code and no remote origins", () => {
    expect(APP_CONTENT_SECURITY_POLICY).not.toContain("unsafe");
    expect(APP_CONTENT_SECURITY_POLICY).not.toMatch(/https?:/);
    expect(APP_CONTENT_SECURITY_POLICY).toContain("default-src 'none'");
  });

  it("knows the bundled asset types", () => {
    expect(contentType("/x/index.html")).toBe("text/html; charset=utf-8");
    expect(contentType("/x/font.WOFF2")).toBe("font/woff2");
    expect(contentType("/x/blob.bin")).toBe("application/octet-stream");
  });
});
