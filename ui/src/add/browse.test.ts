import { describe, expect, it } from "vitest";
import type { AppRow, EntranceOverview, EntranceScope } from "../ipc";
import { INVOICE_SCOPE, OVERVIEW } from "../test/entrances";
import { projectRow } from "../test/workspace";
import {
  applicationSections,
  categoryId,
  entranceSections,
  findApplication,
  headingHeight,
  navItems,
  PER_CATEGORY,
  scopeId,
  selectableNav,
  type Overview,
  type Scope,
} from "./browse";

const ORDERS = "p-orders0000";
const PAYMENTS = "p-payments00";

function overview(): EntranceOverview {
  return OVERVIEW;
}

function invoiceScope(): EntranceScope {
  return INVOICE_SCOPE;
}

const ready: Overview = { status: "ready", row: projectRow(ORDERS, "pathway-agents"), overview: overview() };
const loading: Overview = { status: "loading", row: projectRow(PAYMENTS, "payments") };
const INVOICE: Scope = { kind: "application", project: ORDERS, application: "invoice-hitl" };

function invoiceApp(): AppRow {
  const app = findApplication([ready], INVOICE);
  if (app === null) {
    throw new Error("no Invoice HITL API");
  }
  return app;
}

describe("LOOK IN", () => {
  it("lists All projects and each project, unfolding the expanded one by category", () => {
    const items = navItems([ready, loading], new Set([ORDERS]), new Set(), INVOICE);
    expect(items.map((item) => [item.type, "count" in item ? item.count : null])).toEqual([
      ["all", null],
      ["project", 22],
      ["label", null],
      ["application", null],
      ["label", null],
      ["application", null],
      ["label", null],
      ["application", null],
      ["label", null],
      ["application", null],
      ["unreached", 2],
      ["project", null],
    ]);
    const needsLook = items[3];
    expect(needsLook?.type === "application" && needsLook.warn).toBe(true);
    expect(selectableNav(items).some((item) => item.type === "label")).toBe(false);
  });

  it("totals All projects once every project's overview loaded", () => {
    const payments: Overview = { status: "ready", row: projectRow(PAYMENTS, "payments"), overview: { ...overview(), project: PAYMENTS } };
    expect(navItems([ready, payments], new Set(), new Set(), { kind: "all" })[0]).toMatchObject({ type: "all", count: 44 });
  });

  it("folds a long category into N more until it is opened or a hidden application is selected", () => {
    const many = Array.from({ length: PER_CATEGORY + 3 }, (_, index): AppRow => ({ ...invoiceApp(), id: `app-${index}`, name: `App ${index}` }));
    const long: Overview = { ...ready, overview: { ...overview(), categories: [{ category: "services", applications: many }], unreached: 0 } };
    const folded = navItems([long], new Set([ORDERS]), new Set(), { kind: "all" });
    expect(folded.at(-1)).toMatchObject({ type: "more", count: 3 });
    expect(folded.filter((item) => item.type === "application")).toHaveLength(PER_CATEGORY);
    const opened = navItems([long], new Set([ORDERS]), new Set([categoryId(ORDERS, "services")]), { kind: "all" });
    expect(opened.filter((item) => item.type === "application")).toHaveLength(many.length);
    const hidden = navItems([long], new Set([ORDERS]), new Set(), { kind: "application", project: ORDERS, application: "app-6" });
    expect(hidden.some((item) => item.type === "more")).toBe(false);
  });

  it("says why a project shows no applications", () => {
    const failed: Overview = { status: "error", row: projectRow(PAYMENTS, "payments"), message: "Not compiled yet." };
    expect(navItems([failed], new Set([PAYMENTS]), new Set(), { kind: "all" }).at(-1)).toMatchObject({ type: "note", text: "Not compiled yet." });
  });
});

describe("the list", () => {
  it("puts the launch first, then HTTP by path with mount notes, and a continuation while more rows exist", () => {
    const sections = entranceSections(ORDERS, [invoiceScope()], invoiceApp());
    expect(sections.map((section) => section.heading)).toMatchObject([
      { caps: "Launch", sub: null, first: true },
      { caps: "HTTP", sub: "/runs", mono: true, note: null },
      { caps: null, sub: "/dashboard-builder", note: "also in Dashboard builder", title: "dashboard_builder/router.py" },
    ]);
    expect(sections.map((section) => headingHeight(section.heading))).toEqual([24, 50, 24]);
    const [launch] = sections[0]?.items ?? [];
    expect(launch?.type === "entrance" && launch.launch?.label).toBe("Start API");
    expect(sections.at(-1)?.items.at(-1)).toEqual({ type: "more", id: "more", remaining: 10 });
  });

  it("joins the next page's rows onto their section", () => {
    const first = invoiceScope();
    const second: EntranceScope = {
      ...first,
      rows: first.rows.slice(4).map((row) => ({ ...row, key: "entrance:schema", trigger: "/schema" })),
      nextCursor: null,
    };
    const sections = entranceSections(ORDERS, [first, second], invoiceApp());
    expect(sections).toHaveLength(3);
    expect(sections[2]?.items.map((item) => item.type)).toEqual(["entrance", "entrance", "entrance"]);
  });

  it("lists the applications of All projects by project and category, ending with the unreached", () => {
    const sections = applicationSections([ready, loading], true);
    expect(sections[0]?.heading).toMatchObject({ caps: "pathway-agents", sub: "Needs a look", first: true });
    expect(sections[1]?.heading).toMatchObject({ caps: null, sub: "Services" });
    expect(sections.flatMap((section) => section.items).map((item) => item.id)).toEqual([
      scopeId({ kind: "application", project: ORDERS, application: "dashboard-builder" }),
      scopeId(INVOICE),
      scopeId({ kind: "application", project: ORDERS, application: "review-worker" }),
      scopeId({ kind: "application", project: ORDERS, application: "purchase-eval" }),
      scopeId({ kind: "unreached", project: ORDERS }),
    ]);
  });
});
