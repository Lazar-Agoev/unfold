import { describe, expect, it } from "vitest";
import { parseLink, resolveLink } from "./links";

describe("parseLink", () => {
  it("reads a link to an item, a page, and a board", () => {
    expect(parseLink("unfold://ts-orders/.unfold/boards/order-placement.json?page=page-1&item=create-order")).toEqual({
      ok: true,
      link: { project: "ts-orders", board: ".unfold/boards/order-placement.json", page: "page-1", item: "create-order" },
    });
    expect(parseLink("  unfold://ts-orders/.unfold/boards/order-placement.json?page=page-2\n")).toEqual({
      ok: true,
      link: { project: "ts-orders", board: ".unfold/boards/order-placement.json", page: "page-2" },
    });
    expect(parseLink("UNFOLD://shop/.unfold/boards/b.json")).toEqual({ ok: true, link: { project: "shop", board: ".unfold/boards/b.json" } });
  });

  it("decodes each part", () => {
    expect(parseLink("unfold://my%20shop%23web/.unfold/boards/caf%C3%A9.json?page=page%201&item=a%26b")).toEqual({
      ok: true,
      link: { project: "my shop#web", board: ".unfold/boards/café.json", page: "page 1", item: "a&b" },
    });
  });

  it("says why text isn't a link", () => {
    const cases: [string, string][] = [
      ["https://ts-orders/x.json", "doesn't start with unfold://"],
      ["unfold:///.unfold/boards/b.json", "names no project"],
      ["unfold://ts-orders", "names no board"],
      ["unfold://ts-orders/", "names no board"],
      ["unfold://ts-orders/a//b.json", "names no board"],
      ["unfold://ts-orders/b.json?page=", "page=<id> or item=<id>"],
      ["unfold://ts-orders/b.json?pg=page-1", "page=<id> or item=<id>"],
      ["unfold://ts-orders/b.json?page=a&page=b", "once each"],
      ["unfold://ts-orders/b.json?item=create-order", "names an item but not its page"],
      ["unfold://ts%E0/b.json", "malformed % escape"],
      ["unfold://ts/b.json?page=%E0", "malformed % escape"],
    ];
    for (const [text, why] of cases) {
      const parsed = parseLink(text);
      expect(parsed.ok, text).toBe(false);
      if (!parsed.ok) {
        expect(parsed.message).toContain(why);
        expect(parsed.message).toContain("unfold://<project>/<board>?page=<page>&item=<item>");
      }
    }
  });

  it("quotes long text short", () => {
    const parsed = parseLink(`unfold://${"x".repeat(40_000)}`);
    expect(parsed.ok).toBe(false);
    if (!parsed.ok) {
      expect(parsed.message).toMatch(/^“unfold:\/\/x+…” isn't an Unfold link: it names no board\./);
      expect(parsed.message.length).toBeLessThan(300);
    }
  });
});

describe("resolveLink", () => {
  const projects = [
    { id: "p-orders0000", name: "ts-orders", path: "/code/main/ts-orders" },
    { id: "p-orders2000", name: "ts-orders", path: "/code/feature/ts-orders" },
    { id: "p-payments00", name: "payments", path: "/code/payments" },
  ];

  it("names a project by its folder's name, or by its id", () => {
    expect(resolveLink({ project: "payments", board: "b.json", page: "page-1" }, projects)).toEqual({
      ok: true,
      link: { project: "p-payments00", board: "b.json", page: "page-1" },
    });
    expect(resolveLink({ project: "p-orders2000", board: "b.json" }, projects)).toEqual({ ok: true, link: { project: "p-orders2000", board: "b.json" } });
  });

  it("refuses a folder name two projects share, naming each one's id and folder", () => {
    const resolved = resolveLink({ project: "ts-orders", board: "b.json", page: "page-1" }, projects);
    expect(resolved).toEqual({
      ok: false,
      message:
        "The link can't name one project: 2 projects have a folder named “ts-orders”: p-orders0000 (/code/main/ts-orders), p-orders2000 (/code/feature/ts-orders). Ask the person to copy the link again (it then names the project by its id), or pass project (its id) with board and page.",
    });
  });

  it("says when no project has the name, quoting it short", () => {
    const resolved = resolveLink({ project: "shop", board: "b.json" }, projects);
    expect(resolved).toMatchObject({ ok: false });
    expect(!resolved.ok && resolved.message).toContain("No project named or with the id “shop” is in the workspace");
    const long = resolveLink({ project: "x".repeat(40_000), board: "b.json" }, projects);
    expect(!long.ok && long.message.length).toBeLessThan(300);
  });
});
