/**
 * Add's entrance browser before typing: LOOK IN (All projects, then each project with its applications
 * under their categories and "Not reached from a launch"), and the list of the selected scope (an
 * application's launches and entrances by section, or the applications of one project or of every
 * project). Pure: the view and the keyboard both walk what this builds.
 */

import type { AppRow, CatalogRow, Category, EntranceOverview, EntranceScope, LaunchRow, ProjectId, ProjectRow } from "../ipc/generated";

/** What LOOK IN has selected. */
export type Scope =
  | { readonly kind: "all" }
  | { readonly kind: "project"; readonly project: ProjectId }
  | { readonly kind: "application"; readonly project: ProjectId; readonly application: string }
  | { readonly kind: "unreached"; readonly project: ProjectId };

/** One project's `entrances` overview, as far as it has loaded. */
export type Overview =
  | { readonly status: "loading"; readonly row: ProjectRow }
  | { readonly status: "error"; readonly row: ProjectRow; readonly message: string }
  | { readonly status: "ready"; readonly row: ProjectRow; readonly overview: EntranceOverview };

export const CATEGORY_LABELS: Readonly<Record<Category, string>> = {
  needs_look: "Needs a look",
  services: "Services",
  workers: "Workers",
  scheduled: "Scheduled",
};

export const UNREACHED_NAME = "Not reached from a launch";

/** Applications a category shows in LOOK IN before "N more" (never "1 more"). */
export const PER_CATEGORY = 4;

/** Joins id parts (U+001F, not NUL: ids go into DOM ids). */
const SEP = "\u001f";

export function scopeId(scope: Scope): string {
  switch (scope.kind) {
    case "all":
      return "all";
    case "project":
      return `p${SEP}${scope.project}`;
    case "application":
      return `a${SEP}${scope.project}${SEP}${scope.application}`;
    case "unreached":
      return `u${SEP}${scope.project}`;
  }
}

/** The project a scope is in (null for All projects). */
export function scopeProject(scope: Scope): ProjectId | null {
  return scope.kind === "all" ? null : scope.project;
}

// ---- LOOK IN ----

export type NavItem =
  | { readonly type: "all"; readonly id: string; readonly scope: Scope; readonly count: number | null }
  | { readonly type: "project"; readonly id: string; readonly scope: Scope; readonly row: ProjectRow; readonly count: number | null; readonly expanded: boolean }
  | { readonly type: "label"; readonly id: string; readonly category: Category }
  | { readonly type: "application"; readonly id: string; readonly scope: Scope; readonly app: AppRow; readonly warn: boolean }
  | { readonly type: "unreached"; readonly id: string; readonly scope: Scope; readonly count: number }
  | { readonly type: "more"; readonly id: string; readonly category: string; readonly count: number }
  | { readonly type: "note"; readonly id: string; readonly text: string };

/** The id of a category's "N more" (and of the category in `openCategories`). */
export function categoryId(project: ProjectId, category: Category): string {
  return `c${SEP}${project}${SEP}${category}`;
}

/** The project's unique entrances, once its overview loaded. */
function projectCount(overview: Overview): number | null {
  return overview.status === "ready" ? overview.overview.counts.entrances : null;
}

function projectNav(overview: Overview, openCategories: ReadonlySet<string>, selected: string): NavItem[] {
  const project = overview.row.id;
  if (overview.status === "loading") {
    return [{ type: "note", id: `n${SEP}${project}`, text: "Loading entrances…" }];
  }
  if (overview.status === "error") {
    return [{ type: "note", id: `n${SEP}${project}`, text: overview.message }];
  }
  const { categories, unreached } = overview.overview;
  const items: NavItem[] = [];
  for (const { category, applications } of categories) {
    const id = categoryId(project, category);
    const apps = applications.map((app): Extract<NavItem, { type: "application" }> => {
      const scope: Scope = { kind: "application", project, application: app.id };
      return { type: "application", id: scopeId(scope), scope, app, warn: category === "needs_look" };
    });
    const hiddenSelected = apps.slice(PER_CATEGORY).some((item) => item.id === selected);
    const collapse = apps.length > PER_CATEGORY + 1 && !openCategories.has(id) && !hiddenSelected;
    items.push({ type: "label", id: `l${SEP}${project}${SEP}${category}`, category });
    items.push(...(collapse ? apps.slice(0, PER_CATEGORY) : apps));
    if (collapse) {
      items.push({ type: "more", id, category: id, count: apps.length - PER_CATEGORY });
    }
  }
  if (unreached > 0) {
    const scope: Scope = { kind: "unreached", project };
    items.push({ type: "unreached", id: scopeId(scope), scope, count: unreached });
  }
  if (items.length === 0) {
    items.push({ type: "note", id: `n${SEP}${project}`, text: "No entrances found." });
  }
  return items;
}

/**
 * LOOK IN's rows in order: All projects, then each project, and under an expanded one its applications
 * by category (a few each, then "N more" until opened or one of the hidden is selected), then the
 * entrances no launch reaches.
 */
export function navItems(overviews: readonly Overview[], expanded: ReadonlySet<ProjectId>, openCategories: ReadonlySet<string>, selected: Scope): NavItem[] {
  const counts = overviews.map(projectCount);
  const loaded = counts.filter((count) => count !== null);
  const all = loaded.length === counts.length ? loaded.reduce((sum, count) => sum + count, 0) : null;
  const allScope: Scope = { kind: "all" };
  const items: NavItem[] = [{ type: "all", id: scopeId(allScope), scope: allScope, count: all }];
  const selectedId = scopeId(selected);
  for (const [index, overview] of overviews.entries()) {
    const scope: Scope = { kind: "project", project: overview.row.id };
    const open = expanded.has(overview.row.id);
    items.push({ type: "project", id: scopeId(scope), scope, row: overview.row, count: counts[index] ?? null, expanded: open });
    if (open) {
      items.push(...projectNav(overview, openCategories, selectedId));
    }
  }
  return items;
}

/** The rows ↑ ↓ walk in LOOK IN. */
export function selectableNav(items: readonly NavItem[]): NavItem[] {
  return items.filter((item) => item.type !== "label" && item.type !== "note");
}

// ---- The list ----

export type ListItem =
  | {
      readonly type: "entrance";
      readonly id: string;
      readonly project: ProjectId;
      readonly row: CatalogRow;
      /** The launch a launch row stands for. */
      readonly launch: LaunchRow | null;
    }
  | { readonly type: "application"; readonly id: string; readonly scope: Scope; readonly app: AppRow; readonly warn: boolean }
  | { readonly type: "unreached"; readonly id: string; readonly scope: Scope; readonly count: number }
  /** The continuation: more rows than are loaded. */
  | { readonly type: "more"; readonly id: string; readonly remaining: number };

/** What a section's heading shows: a caps title (`LAUNCH`, `HTTP`), a sub-heading (`/runs`), and a note. */
export interface Heading {
  readonly caps: string | null;
  readonly sub: string | null;
  /** The sub-heading is a path. */
  readonly mono: boolean;
  /** `also in Dashboard builder`. */
  readonly note: string | null;
  /** The router file the routes are mounted from (the sub-heading's tooltip). */
  readonly title: string | null;
  /** The list's first heading (less room above it). */
  readonly first: boolean;
}

export interface ListSection {
  readonly id: string;
  /** The heading as plain text. */
  readonly label: string;
  readonly heading: Heading;
  readonly items: readonly ListItem[];
}

/** Heading heights, as drawn: the first caps title 24, a later one 30, a sub-heading 24 (20 under a caps title). */
export function headingHeight(heading: Heading): number {
  const caps = heading.caps === null ? 0 : heading.first ? 24 : 30;
  const sub = heading.sub === null ? 0 : heading.caps === null ? 24 : 20;
  return caps + sub;
}

/** Launch rows are 30 tall, the rest 28. */
export function itemHeight(item: ListItem): number {
  return item.type === "entrance" && item.launch !== null ? 30 : 28;
}

function heading(fields: Partial<Heading> & Pick<Heading, "first">): Heading {
  return { caps: null, sub: null, mono: false, note: null, title: null, ...fields };
}

function labelOf(head: Heading): string {
  return [head.caps, head.sub].filter((part) => part !== null).join(" ");
}

/**
 * The sections of a scope's loaded pages: rows grouped by their section (Launch, then HTTP by path
 * prefix, then the other kinds), a caps title where the kind changes, and the continuation after the
 * last row while there is a next page. Launch rows carry their launch from the application.
 */
export function entranceSections(project: ProjectId, pages: readonly EntranceScope[], app: AppRow | null): ListSection[] {
  const last = pages.at(-1);
  if (!last) {
    return [];
  }
  const launches = new Map((app?.launches ?? []).map((launch) => [launch.key, launch]));
  const groups: { index: number; rows: CatalogRow[] }[] = [];
  for (const row of pages.flatMap((page) => page.rows)) {
    const group = groups.at(-1);
    if (group?.index === row.section) {
      group.rows.push(row);
    } else {
      groups.push({ index: row.section, rows: [row] });
    }
  }
  const sections: ListSection[] = [];
  let kind: string | null = null;
  for (const group of groups) {
    const section = last.sections[group.index];
    if (!section) {
      continue;
    }
    const first = sections.length === 0;
    const newKind = section.kind !== kind;
    kind = section.kind;
    const head =
      section.kind === "http"
        ? heading({
            first,
            caps: newKind ? "HTTP" : null,
            sub: section.title,
            mono: true,
            note: section.alsoIn.length > 0 ? `also in ${section.alsoIn.join(", ")}` : null,
            title: section.mountedFrom,
          })
        : heading({ first, caps: section.title });
    const items: ListItem[] = group.rows.map((row) => ({
      type: "entrance",
      id: `e${SEP}${project}${SEP}${row.key}`,
      project,
      row,
      launch: section.kind === "launch" ? (launches.get(row.key) ?? null) : null,
    }));
    sections.push({ id: `s${SEP}${group.index}`, label: labelOf(head), heading: head, items });
  }
  const loaded = pages.reduce((sum, page) => sum + page.rows.length, 0);
  const end = sections.at(-1);
  if (last.nextCursor !== null && end) {
    sections[sections.length - 1] = { ...end, items: [...end.items, { type: "more", id: "more", remaining: Math.max(0, last.total - loaded) }] };
  }
  return sections;
}

/**
 * The applications of the shown projects (All projects, or one project), by project and category, as
 * LOOK IN groups them, each project ending with the entrances no launch reaches.
 */
export function applicationSections(overviews: readonly Overview[], showProjects: boolean): ListSection[] {
  const sections: ListSection[] = [];
  for (const overview of overviews) {
    if (overview.status !== "ready") {
      continue;
    }
    const project = overview.row.id;
    const { categories, unreached } = overview.overview;
    const projectSections: ListSection[] = categories.map(({ category, applications }, index) => {
      const head = heading({ first: sections.length === 0 && index === 0, caps: showProjects && index === 0 ? overview.row.name : null, sub: CATEGORY_LABELS[category] });
      const items = applications.map((app): ListItem => {
        const scope: Scope = { kind: "application", project, application: app.id };
        return { type: "application", id: scopeId(scope), scope, app, warn: category === "needs_look" };
      });
      return { id: categoryId(project, category), label: labelOf(head), heading: head, items };
    });
    if (unreached > 0) {
      const scope: Scope = { kind: "unreached", project };
      const item: ListItem = { type: "unreached", id: scopeId(scope), scope, count: unreached };
      const end = projectSections.at(-1);
      if (end) {
        projectSections[projectSections.length - 1] = { ...end, items: [...end.items, item] };
      } else {
        const head = heading({ first: sections.length === 0, caps: showProjects ? overview.row.name : null });
        projectSections.push({ id: `u${SEP}${project}`, label: labelOf(head), heading: head, items: [item] });
      }
    }
    sections.push(...projectSections);
  }
  return sections;
}

/** The application a scope names, from its project's overview. */
export function findApplication(overviews: readonly Overview[], scope: Scope): AppRow | null {
  if (scope.kind !== "application") {
    return null;
  }
  const overview = overviews.find((entry) => entry.row.id === scope.project);
  if (overview?.status !== "ready") {
    return null;
  }
  for (const group of overview.overview.categories) {
    const app = group.applications.find((entry) => entry.id === scope.application);
    if (app) {
      return app;
    }
  }
  return null;
}
