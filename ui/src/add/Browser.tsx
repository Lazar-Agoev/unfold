/**
 * Add's entrance browser before typing: LOOK IN (All projects, each project with its applications by
 * category), and the list of the selected scope under its header: an application's launches and
 * entrances in aligned columns (kind, trigger, handler) with an "on page" mark, or the applications of
 * a project or of every project. Focus stays in Add's input; it points into whichever pane the keyboard
 * is in with `aria-activedescendant`.
 */

import type { ReactNode } from "react";
import { Monogram } from "../components";
import { Icon } from "../icons";
import type { ProjectId } from "../ipc";
import { methodColumn } from "../lib/entrances";
import { formatCount, plural } from "../lib/format";
import { cardRefKey } from "../state/boards";
import { CATEGORY_LABELS, UNREACHED_NAME, headingHeight, itemHeight, type Heading, type ListItem, type ListSection, type NavItem } from "./browse";
import { ResultList } from "./ResultList";

/** The DOM id of LOOK IN's row at `index` (for `aria-activedescendant`). */
export function navOptionId(id: string, index: number): string {
  return `${id}-${index}`;
}

interface LookInProps {
  readonly id: string;
  readonly items: readonly NavItem[];
  /** The selected scope's id. */
  readonly selected: string;
  /** The keyboard's row, while the keyboard is in LOOK IN. */
  readonly cursor: string | null;
  readonly onChoose: (item: NavItem) => void;
  readonly onToggle: (project: ProjectId) => void;
}

function Count({ value }: { readonly value: number | null }) {
  return <span className="add-nav-count">{value === null ? "" : formatCount(value)}</span>;
}

function navContent(item: NavItem, onToggle: (project: ProjectId) => void): ReactNode {
  switch (item.type) {
    case "all":
      return (
        <>
          <Icon name="layers" className="add-nav-icon" />
          <span className="add-nav-name">All projects</span>
          <Count value={item.count} />
        </>
      );
    case "project":
      return (
        <>
          <span
            className="add-nav-chevron"
            aria-hidden="true"
            onClick={(event) => {
              event.stopPropagation();
              onToggle(item.row.id);
            }}
          >
            <Icon name={item.expanded ? "chevron-down-small" : "chevron-right-small"} size={10} />
          </span>
          <Monogram name={item.row.name} size={20} />
          <span className="add-nav-name">{item.row.name}</span>
          <Count value={item.count} />
        </>
      );
    case "label":
      return CATEGORY_LABELS[item.category];
    case "application":
      return (
        <>
          <span className="add-nav-name">{item.app.name}</span>
          {item.warn ? <span className="add-nav-warn" aria-label="Needs a look" /> : <Count value={item.app.entrances} />}
        </>
      );
    case "unreached":
      return (
        <>
          <span className="add-nav-name">{UNREACHED_NAME}</span>
          <Count value={item.count} />
        </>
      );
    case "more":
      return <span className="add-nav-name">{`${formatCount(item.count)} more`}</span>;
    case "note":
      return item.text;
  }
}

function navClass(item: NavItem, selected: boolean, cursor: boolean): string {
  const classes = ["add-nav-row", item.type];
  if (item.type === "label" && item.category === "needs_look") {
    classes.push("warn");
  }
  if (selected) {
    classes.push("selected");
  }
  if (cursor) {
    classes.push("cursor");
  }
  return classes.join(" ");
}

/** LOOK IN: a tree of scopes; a click selects, a project's chevron folds it. */
export function LookIn({ id, items, selected, cursor, onChoose, onToggle }: LookInProps) {
  return (
    <div className="add-lookin">
      <div className="add-lookin-label" id={`${id}-label`}>
        Look in
      </div>
      <div role="tree" aria-labelledby={`${id}-label`} className="add-lookin-rows">
        {items.map((item, index) => {
          if (item.type === "label" || item.type === "note") {
            return (
              <div key={item.id} role="presentation" className={navClass(item, false, false)}>
                {navContent(item, onToggle)}
              </div>
            );
          }
          const isSelected = item.id === selected;
          const level = item.type === "all" || item.type === "project" ? 1 : 2;
          return (
            <div
              key={item.id}
              id={navOptionId(id, index)}
              role="treeitem"
              aria-level={level}
              aria-selected={isSelected}
              aria-expanded={item.type === "project" ? item.expanded : undefined}
              className={navClass(item, isSelected, item.id === cursor)}
              onMouseDown={(event) => {
                event.preventDefault();
              }}
              onClick={() => {
                onChoose(item);
              }}
            >
              {navContent(item, onToggle)}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** The list's header: the scope's name, its counts, and its description. */
export interface ListHeader {
  readonly title: string;
  readonly meta: string | null;
  readonly description: string | null;
}

interface EntranceListProps {
  readonly id: string;
  readonly header: ListHeader | null;
  readonly sections: readonly ListSection[];
  /** Shown instead of the rows (loading, an error, nothing). */
  readonly message: string | null;
  readonly selected: string | null;
  /** `cardRefKey`s of the cards on the page Add adds to. */
  readonly onPage: ReadonlySet<string>;
  readonly blocked: (project: ProjectId) => string | null;
  readonly loadingMore: boolean;
  readonly onActivate: (item: ListItem, newPage: boolean) => void;
  readonly onEnd: () => void;
}

/** Whether the entrance is on the page Add adds to. */
function isOnPage(item: ListItem, onPage: ReadonlySet<string>): boolean {
  return item.type === "entrance" && onPage.has(cardRefKey(item.project, item.row.key));
}

/** A row's accessible name. */
function itemText(item: ListItem, onPage: boolean, loadingMore: boolean): string {
  switch (item.type) {
    case "entrance": {
      const { row, launch } = item;
      const text = launch ? ["Launch", launch.label, launch.command ?? row.title].filter((part) => part !== null).join(" ") : `${methodColumn(row)} ${row.trigger ?? row.title}`;
      return onPage ? `${text} on page` : text;
    }
    case "application":
      return `${item.app.name}, ${plural(item.app.entrances, "entrance")}`;
    case "unreached":
      return `${UNREACHED_NAME}, ${plural(item.count, "entrance")}`;
    case "more":
      return moreText(item.remaining, loadingMore);
  }
}

function moreText(remaining: number, loading: boolean): string {
  return loading ? "Loading more…" : `${formatCount(remaining)} more`;
}

function OnPage() {
  return (
    <span className="add-entry-on-page">
      <span className="add-entry-dot" aria-hidden="true" />
      on page
    </span>
  );
}

function Entry({ item, onPage, blocked, loadingMore }: { readonly item: ListItem; readonly onPage: boolean; readonly blocked: string | null; readonly loadingMore: boolean }) {
  switch (item.type) {
    case "entrance": {
      const { row, launch } = item;
      const classes = ["add-entry", launch ? "launch" : null, blocked === null ? null : "blocked"].filter(Boolean).join(" ");
      if (launch) {
        return (
          <div className={classes} title={blocked ?? undefined}>
            <span className="add-entry-kind">LAUNCH</span>
            {launch.label === null ? null : <span className="add-entry-label">{launch.label}</span>}
            <span className="add-entry-command add-code">{launch.command ?? row.title}</span>
            {onPage ? <OnPage /> : null}
            {launch.problem === null ? null : <span className="add-entry-problem">{launch.problem}</span>}
          </div>
        );
      }
      const method = methodColumn(row);
      return (
        <div className={classes} title={blocked ?? undefined}>
          <span className={method === "DELETE" ? "add-entry-kind danger" : "add-entry-kind"}>{method}</span>
          <span className="add-entry-trigger add-code">{row.trigger ?? row.title}</span>
          {onPage ? <OnPage /> : null}
          <span className="add-entry-handler add-code">{row.handler ?? ""}</span>
        </div>
      );
    }
    case "application":
      return (
        <div className="add-entry app">
          <span className="add-entry-name">{item.app.name}</span>
          {item.warn ? <span className="add-nav-warn" aria-hidden="true" /> : null}
          <span className="add-entry-count">{plural(item.app.entrances, "entrance")}</span>
        </div>
      );
    case "unreached":
      return (
        <div className="add-entry app">
          <span className="add-entry-name">{UNREACHED_NAME}</span>
          <span className="add-entry-count">{plural(item.count, "entrance")}</span>
        </div>
      );
    case "more":
      return <div className="add-entry more">{moreText(item.remaining, loadingMore)}</div>;
  }
}

function SectionHeading({ heading }: { readonly heading: Heading }) {
  return (
    <div className={heading.first ? "add-heading first" : "add-heading"}>
      {heading.caps === null ? null : <span className="add-heading-caps">{heading.caps}</span>}
      {heading.sub === null ? null : (
        <span className="add-heading-sub" title={heading.title ?? undefined}>
          <span className={heading.mono ? "add-code" : undefined}>{heading.sub}</span>
          {heading.note === null ? null : <span className="add-heading-note">{` · ${heading.note}`}</span>}
        </span>
      )}
    </div>
  );
}

function itemProject(item: ListItem): ProjectId | null {
  return item.type === "entrance" ? item.project : null;
}

/** The middle pane: the header, then the rows (virtualized, with a continuation at the end). */
export function EntranceList({ id, header, sections, message, selected, onPage, blocked, loadingMore, onActivate, onEnd }: EntranceListProps) {
  const blockedOf = (item: ListItem) => {
    const project = itemProject(item);
    return project === null ? null : blocked(project);
  };
  return (
    <div className="add-browse-list">
      {header === null ? null : (
        <header className="add-browse-header">
          <div className="add-browse-title-row">
            <h3 className="add-browse-title">{header.title}</h3>
            {header.meta === null ? null : <span className="add-browse-meta">{header.meta}</span>}
          </div>
          {header.description === null ? null : <p className="add-browse-description">{header.description}</p>}
        </header>
      )}
      {message !== null ? (
        <p className="add-empty">{message}</p>
      ) : (
        <ResultList<ListItem, ListSection>
          id={id}
          label="Entrances"
          className="add-browse-rows"
          sections={sections}
          getId={(item) => item.id}
          getHeight={itemHeight}
          getText={(item) => itemText(item, isOnPage(item, onPage), loadingMore)}
          renderHeader={(section) => <SectionHeading heading={section.heading} />}
          getHeaderHeight={(section) => headingHeight(section.heading)}
          selectedId={selected}
          onActivate={(item, event) => {
            onActivate(item, event.metaKey || event.ctrlKey);
          }}
          onEnd={onEnd}
          renderItem={(item) => <Entry item={item} onPage={isOnPage(item, onPage)} blocked={blockedOf(item)} loadingMore={loadingMore} />}
        />
      )}
    </div>
  );
}
