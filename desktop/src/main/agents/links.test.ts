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
    expect(parseLink("UNFOLD://shop/.unfold/local/drafts/b.json")).toEqual({ ok: true, link: { project: "shop", board: ".unfold/local/drafts/b.json" } });
  });

  it("decodes each part", () => {
    expect(parseLink("unfold://my%20shop%2Fweb/.unfold/boards/caf%C3%A9.json?page=page%201&item=a%26b")).toEqual({
      ok: true,
      link: { project: "my shop/web", board: ".unfold/boards/café.json", page: "page 1", item: "a&b" },
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
});

describe("resolveLink", () => {
  const projects = [
    { id: "p-orders0000", name: "ts-orders" },
    { id: "p-orders2000", name: "ts-orders" },
    { id: "p-payments00", name: "payments" },
  ];

  it("names the first project with the link's shared name", () => {
    expect(resolveLink({ project: "ts-orders", board: "b.json", page: "page-1" }, projects)).toEqual({
      ok: true,
      link: { project: "p-orders0000", board: "b.json", page: "page-1" },
    });
  });

  it("says when no project has the name", () => {
    const resolved = resolveLink({ project: "shop", board: "b.json" }, projects);
    expect(resolved).toMatchObject({ ok: false });
    expect(!resolved.ok && resolved.message).toContain("No project named “shop” is in the workspace");
  });
});
