/**
 * The Code view of a card (docs/reader.md#code-view, 02 §4.3): the exact verified slice with syntax colors in
 * 20px rows, folds from the gutter, calls as links, board groups on a magenta rail, and a port at the
 * card's edge for each calling line that has an arrow (the row gets the step fill). While a line
 * between cards is selected, the callee's lines its contract reads get the step fill, and the caller's
 * calling line the selection fill.
 *
 * The component only lays out the reader's `CodeText` model; it never re-slices or re-parses.
 * Ports are drawn by the caller (`renderPorts`), so the renderer stays independent of the canvas.
 */

import { Fragment, useMemo, useState, type MouseEvent, type ReactNode } from "react";
import type { CallLink as CallLinkModel, CodeLine, CodeText as CodeTextModel, Group, Language, OutlineItem, ProjectId } from "../ipc/generated";
import { Icon } from "../icons";
import { CallLink } from "./CallLink";
import { CodeText } from "./CodeText";
import { foldHiding, innermostItem, lineAt, outlineChildren, portStep } from "./outline";
import { linkRanges, spanRanges } from "./segments";
import "./code.css";

export interface CodeViewProps {
  /** The project the code is in (a call's hover asks it about the target). */
  readonly project: ProjectId;
  readonly code: CodeTextModel;
  readonly language: Language;
  /** The card's name, for the view's accessible label. */
  readonly label: string;
  readonly folds: ReadonlySet<string>;
  readonly onToggleFold?: (key: string) => void;
  /** The board's groups inside this card, unbroken ones only. */
  readonly groups?: readonly Group[];
  readonly onToggleGroup?: (groupId: string) => void;
  /** Renames a group from its rail (double-click or F2 on the title). */
  readonly onRenameGroup?: (groupId: string, title: string) => void;
  /** Opens a group's menu (right-click on its rail, or Shift+F10 on its toggle). */
  readonly onGroupMenu?: (groupId: string, at: MouseEvent | Element) => void;
  /** Occurrence keys with an arrow; each gets a port at the end of its line (or the row hiding it). */
  readonly ports?: ReadonlySet<string>;
  readonly renderPorts?: (ids: readonly string[]) => ReactNode;
  readonly selectedKey?: string | null;
  readonly onSelect?: (key: string) => void;
  readonly onFollow?: (link: CallLinkModel, anchor: HTMLElement) => void;
  readonly onExpand?: (link: CallLinkModel) => void;
  /** Callees expanded inline, by occurrence key, drawn under the calling line. */
  readonly expanded?: ReadonlyMap<string, ReactNode>;
  /** The last verified render of source that changed: shown dimmed, never re-sliced. */
  readonly dimmed?: boolean;
  /** Lines marked with the step fill: the callee lines a selected line's contract reads. */
  readonly marked?: ReadonlySet<number>;
  /** The calling line of a selected line, in the selection fill. */
  readonly callerLine?: number | null;
}

const EMPTY_SET: ReadonlySet<string> = new Set();

interface RowInfo {
  readonly line: CodeLine;
  /** Characters of indentation the view hides (the model's `dedent`). */
  readonly skip: number;
}

/**
 * Where a port's line is, and where on it: an occurrence's (its call's underline), or a connection's
 * step (a call, else the statement that is or holds it); undefined for a port the text doesn't show.
 */
export function portSpot(code: CodeTextModel, id: string): { readonly line: number; readonly offset: number } | undefined {
  const step = portStep(id);
  const key = step ?? id;
  const link = code.links.find((candidate) => candidate.source === key);
  if (link) {
    return { line: link.line, offset: link.span?.[0] ?? code.lines.find((line) => line.number === link.line)?.start ?? 0 };
  }
  const item = step !== undefined ? innermostItem(code.outline, step) : undefined;
  return item ? { line: item.startLine, offset: item.start } : undefined;
}

/** Where each port sits: `line:N`, `fold:<key>` (a folded row hides it), or `group:<id>` (a collapsed group). */
export function portAnchors(
  code: CodeTextModel,
  ports: ReadonlySet<string>,
  folds: ReadonlySet<string>,
  groups: readonly Group[],
): Map<string, string[]> {
  const anchors = new Map<string, string[]>();
  const byKey = new Map(code.outline.map((item) => [item.key, item]));
  for (const id of ports) {
    const spot = portSpot(code, id);
    if (!spot) {
      continue;
    }
    const { offset } = spot;
    const group = groups.find(
      (candidate) =>
        candidate.collapsed &&
        candidate.members.some((key) => {
          const member = byKey.get(key);
          return member !== undefined && offset >= member.start && offset < member.end;
        }),
    );
    const fold = group ? undefined : foldHiding(code.outline, folds, offset);
    const anchor = group ? `group:${group.id}` : fold ? `fold:${fold.key}` : `line:${spot.line}`;
    const list = anchors.get(anchor) ?? [];
    list.push(id);
    anchors.set(anchor, list);
  }
  return anchors;
}

export function CodeView(props: CodeViewProps) {
  const { code, folds, groups = [], ports = EMPTY_SET, expanded } = props;
  const spans = useMemo(() => spanRanges(code.spans), [code.spans]);
  const links = useMemo(() => linkRanges(code.links), [code.links]);
  const children = useMemo(() => outlineChildren(code.outline), [code.outline]);
  const rows = useMemo(() => {
    const map = new Map<number, RowInfo>();
    code.lines.forEach((line, index) => {
      map.set(line.number, { line, skip: index === 0 ? 0 : Math.min(code.dedent, line.indent) });
    });
    return map;
  }, [code.lines, code.dedent]);
  // The innermost outline item on each line, so clicking a row selects its statement.
  const ownerOfLine = useMemo(() => {
    const owners = new Map<number, string>();
    for (const item of code.outline) {
      for (let line = item.startLine; line <= item.endLine; line += 1) {
        owners.set(line, item.key);
      }
    }
    return owners;
  }, [code.outline]);
  const digits = String(code.lines.at(-1)?.number ?? 1).length;
  const selected = props.selectedKey ? code.outline.find((item) => item.key === props.selectedKey) : undefined;
  const anchors = useMemo(() => portAnchors(code, ports, folds, groups), [code, ports, folds, groups]);
  const expandedAt = useMemo(() => {
    const byLine = new Map<number, ReactNode[]>();
    for (const [id, node] of expanded ?? []) {
      const link = code.links.find((candidate) => candidate.source === id);
      if (link) {
        const list = byLine.get(link.line) ?? [];
        list.push(<Fragment key={id}>{node}</Fragment>);
        byLine.set(link.line, list);
      }
    }
    return byLine;
  }, [expanded, code.links]);

  // The board's groups, by member.
  const groupOf = useMemo(() => {
    const map = new Map<string, Group>();
    for (const group of groups) {
      for (const key of group.members) {
        map.set(key, group);
      }
    }
    return map;
  }, [groups]);

  // The group whose title is being renamed on its rail.
  const [renaming, setRenaming] = useState<string | null>(null);

  const renderLink = (index: number, content: ReactNode): ReactNode => {
    const link = code.links[index];
    if (!link) {
      return content;
    }
    return (
      <CallLink
        project={props.project}
        link={link}
        opened={ports.has(link.source)}
        {...(props.onFollow ? { onFollow: props.onFollow } : {})}
        {...(props.onExpand ? { onExpand: props.onExpand } : {})}
      >
        {content}
      </CallLink>
    );
  };
  const text = (start: number, end: number) => (
    <CodeText text={code.text} start={start} end={end} spans={spans} links={links} renderLink={renderLink} />
  );
  const portsFor = (anchor: string): ReactNode => {
    const ids = anchors.get(anchor);
    if (!ids || ids.length === 0 || !props.renderPorts) {
      return null;
    }
    return props.renderPorts(ids);
  };
  const select = (key: string | undefined) => (event: MouseEvent) => {
    if (key !== undefined && props.onSelect) {
      event.stopPropagation();
      props.onSelect(key);
    }
  };
  // The line number, right-aligned in the gutter; a fold's chevron sits in the space after it.
  const gutter = (number: number, chevron: { key: string; open: boolean } | undefined) => (
    <span className="code-gutter">
      <span className="code-number nodrag">{number}</span>
      {chevron && props.onToggleFold ? (
        <button
          type="button"
          tabIndex={-1}
          className={`code-chevron nodrag${chevron.open ? "" : " folded"}`}
          aria-label={`${chevron.open ? "Fold" : "Unfold"} line ${number}`}
          aria-expanded={chevron.open}
          onClick={(event) => {
            event.stopPropagation();
            props.onToggleFold?.(chevron.key);
          }}
        >
          <Icon name={chevron.open ? "chevron-down-small" : "chevron-right-small"} size={10} />
        </button>
      ) : null}
    </span>
  );
  const isSelectedLine = (number: number) => selected !== undefined && number >= selected.startLine && number <= selected.endLine;

  function row(number: number, chevron?: string): ReactNode {
    const info = rows.get(number);
    if (!info) {
      return null;
    }
    const { line, skip } = info;
    const anchor = `line:${number}`;
    const classes = ["code-row"];
    if (anchors.has(anchor)) classes.push("has-port");
    if (props.marked?.has(number)) classes.push("marked");
    if (isSelectedLine(number) || props.callerLine === number) classes.push("selected");
    return (
      <Fragment key={`r${number}`}>
        <div className={classes.join(" ")} data-line={number} data-port-row="code" onClick={select(ownerOfLine.get(number))}>
          {gutter(number, chevron !== undefined ? { key: chevron, open: true } : undefined)}
          <span className="code-text">
            {line.end > line.start + skip ? text(line.start + skip, line.end) : " "}
          </span>
          {portsFor(anchor)}
        </div>
        {expandedAt.get(number)?.map((node, index) => (
          <div key={`x${index}`} className="code-inline nodrag nopan">
            {node}
          </div>
        ))}
      </Fragment>
    );
  }

  function foldRow(item: OutlineItem): ReactNode {
    const fold = item.fold;
    const first = rows.get(item.startLine);
    const last = lineAt(code.lines, Math.max(item.start, item.end - 1));
    if (!fold || !first || !last) {
      return null;
    }
    const prefixStart = first.line.start + first.skip;
    const headerEnd = Math.max(item.start, Math.min(fold.headerEnd, item.end));
    const anchor = `fold:${item.key}`;
    const classes = ["code-row", "folded"];
    if (anchors.has(anchor)) classes.push("has-port");
    if (isSelectedLine(item.startLine)) classes.push("selected");
    const badge = fold.braces ? `{ ⋯ ${fold.summary} }` : `⋯ ${fold.summary}`;
    return (
      <div key={`f${item.key}`} className={classes.join(" ")} data-line={item.startLine} data-block={item.key} data-port-row="code" onClick={select(item.key)}>
        {gutter(item.startLine, { key: item.key, open: false })}
        <span className="code-text">
          {text(prefixStart, headerEnd)}
          {props.language === "python" ? " " : ""}
          <button
            type="button"
            className="code-fold-badge nodrag"
            aria-label={`Unfold ${fold.summary}`}
            onClick={(event) => {
              event.stopPropagation();
              props.onToggleFold?.(item.key);
            }}
          >
            {badge}
          </button>
          {item.end < last.end ? text(item.end, last.end) : null}
        </span>
        {portsFor(anchor)}
      </div>
    );
  }

  function container(index: number, item: OutlineItem, chevrons: readonly string[]): ReactNode {
    if (folds.has(item.key)) {
      return foldRow(item);
    }
    return (
      <div key={`b${item.key}`} className={`code-block${props.selectedKey === item.key ? " selected" : ""}`} data-block={item.key}>
        {range(item.startLine, item.endLine, children.get(index) ?? [], false, [...chevrons, item.key])}
      </div>
    );
  }

  function groupBlock(group: Group, members: readonly OutlineItem[], memberIndexes: readonly number[]): ReactNode {
    const firstMember = members[0];
    const lastLine = members.reduce((end, member) => Math.max(end, member.endLine), firstMember?.endLine ?? 0);
    if (!firstMember) {
      return null;
    }
    const first = rows.get(firstMember.startLine);
    const indent = first ? Math.max(0, first.line.indent - first.skip) : 0;
    const anchor = `group:${group.id}`;
    const finishRename = (value: string | null) => {
      setRenaming(null);
      if (value !== null && value.trim().length > 0 && value.trim() !== group.title) {
        props.onRenameGroup?.(group.id, value.trim());
      }
    };
    return (
      <div key={`g${group.id}`} className={`code-group${group.collapsed ? " collapsed" : ""}`} data-group={group.id}>
        <div
          className={`code-group-heading${group.collapsed && anchors.has(anchor) ? " has-port" : ""}`}
          onContextMenu={
            props.onGroupMenu
              ? (event) => {
                  event.stopPropagation();
                  props.onGroupMenu?.(group.id, event);
                }
              : undefined
          }
        >
          <button
            type="button"
            className="code-group-toggle nodrag"
            aria-label={`${group.collapsed ? "Expand" : "Collapse"} ${group.title}`}
            aria-expanded={!group.collapsed}
            onClick={(event) => {
              event.stopPropagation();
              props.onToggleGroup?.(group.id);
            }}
            onKeyDown={(event) => {
              if ((event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) && props.onGroupMenu) {
                event.preventDefault();
                event.stopPropagation();
                props.onGroupMenu(group.id, event.currentTarget);
              } else if (event.key === "F2" && props.onRenameGroup) {
                event.preventDefault();
                event.stopPropagation();
                setRenaming(group.id);
              }
            }}
          >
            <Icon name={group.collapsed ? "chevron-right-small" : "chevron-down-small"} size={10} />
          </button>
          {renaming === group.id ? (
            <input
              autoFocus
              className="code-group-input nodrag nopan"
              style={{ marginLeft: `${indent}ch` }}
              aria-label={`Rename ${group.title}`}
              defaultValue={group.title}
              onClick={(event) => {
                event.stopPropagation();
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  finishRename(event.currentTarget.value);
                } else if (event.key === "Escape") {
                  finishRename(null);
                }
                event.stopPropagation();
              }}
              onBlur={(event) => {
                finishRename(event.currentTarget.value);
              }}
            />
          ) : (
            <span
              className="code-group-title"
              style={{ marginLeft: `${indent}ch` }}
              title={props.onRenameGroup ? "Double-click to rename" : undefined}
              onDoubleClick={
                props.onRenameGroup
                  ? (event) => {
                      event.stopPropagation();
                      setRenaming(group.id);
                    }
                  : undefined
              }
            >
              {group.title}
            </span>
          )}
          {group.collapsed ? <span className="code-fold-badge static">{lastLine - firstMember.startLine + 1} lines</span> : null}
          {group.collapsed ? portsFor(anchor) : null}
        </div>
        {group.collapsed ? null : range(firstMember.startLine, lastLine, memberIndexes, true, [])}
      </div>
    );
  }

  /**
   * Rows `fromLine`–`toLine`, with containers for the compound items and groups among `items`.
   * Items that start on a line already drawn, or that need no container, give way to their children.
   */
  function range(fromLine: number, toLine: number, items: readonly number[], inGroup: boolean, chevrons: readonly string[]): ReactNode[] {
    const out: ReactNode[] = [];
    let line = fromLine;
    let pendingChevrons = chevrons;
    const emitUntil = (stop: number) => {
      while (line < stop && line <= toLine) {
        out.push(row(line, pendingChevrons.at(-1)));
        pendingChevrons = [];
        line += 1;
      }
    };
    const queue = [...items];
    while (queue.length > 0) {
      const index = queue.shift();
      const item = index !== undefined ? code.outline[index] : undefined;
      if (index === undefined || !item) {
        continue;
      }
      if (item.startLine < line) {
        queue.unshift(...(children.get(index) ?? []));
        continue;
      }
      const group = inGroup ? undefined : groupOf.get(item.key);
      if (group?.members[0] === item.key) {
        const memberIndexes = [index];
        while (queue.length > 0) {
          const next = queue[0];
          const nextItem = next !== undefined ? code.outline[next] : undefined;
          if (next === undefined || !nextItem || !group.members.includes(nextItem.key)) {
            break;
          }
          memberIndexes.push(next);
          queue.shift();
        }
        const members = memberIndexes.map((member) => code.outline[member]).filter((member): member is OutlineItem => member !== undefined);
        emitUntil(item.startLine);
        pendingChevrons = [];
        out.push(groupBlock(group, members, memberIndexes));
        line = members.reduce((end, member) => Math.max(end, member.endLine), item.endLine) + 1;
        continue;
      }
      if (!item.fold) {
        queue.unshift(...(children.get(index) ?? []));
        continue;
      }
      emitUntil(item.startLine);
      const carried = line === item.startLine ? pendingChevrons : [];
      pendingChevrons = [];
      out.push(container(index, item, carried));
      line = item.endLine + 1;
    }
    emitUntil(toLine + 1);
    return out;
  }

  const firstLine = code.lines[0]?.number ?? 1;
  const lastLine = code.lines.at(-1)?.number ?? firstLine;
  return (
    <div
      className={`code-view${props.dimmed ? " dimmed" : ""}`}
      role="group"
      aria-label={`Source of ${props.label}`}
      style={{ ["--code-digits" as string]: `${digits}ch` }}
    >
      {range(firstLine, lastLine, children.get(-1) ?? [], false, [])}
    </div>
  );
}
