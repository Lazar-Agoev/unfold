/**
 * A virtualized tree (Project settings' scope tree). Data-independent: it asks for each item's
 * id, text, and children, and renders rows from slots (icon, label, description, right-aligned quiet
 * marks). Children load lazily: when an expanded item has no children yet, the tree shows a loading
 * row and calls `onLoadChildren` once.
 *
 * Keyboard (WAI-ARIA tree): ↑/↓ move, → expands or moves to the first child, ← collapses or moves to
 * the parent, Home/End and PageUp/PageDown jump, `*` expands the siblings, typing jumps by name,
 * Space selects, Enter opens (or selects), Shift+F10 and the Menu key open the context menu. Focus
 * stays on the tree and follows the active row through `aria-activedescendant`, so rows can scroll
 * out of the DOM.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Icon } from "../icons";
import { cx, isContextMenuKey, pointBelow, type Point } from "./internal/dom";
import "./TreeView.css";

/** An item's children: loaded, loading, or failed. `undefined` from `getChildren` means not requested. */
export type TreeChildren<T> =
  | readonly T[]
  | { readonly status: "loading" }
  | { readonly status: "error"; readonly message: string };

/** What a row shows. */
export interface TreeRowContent {
  readonly label: ReactNode;
  readonly icon?: ReactNode;
  /** Quiet text right after the label ("skipped by default"). */
  readonly description?: ReactNode;
  /** Right-aligned marks (a file count). */
  readonly marks?: ReactNode;
  /** Drawn muted (ignored folders). */
  readonly dimmed?: boolean;
  /** Native tooltip, e.g. a full path. */
  readonly title?: string;
}

export interface TreeRowState {
  readonly depth: number;
  readonly expanded: boolean;
  readonly selected: boolean;
  readonly active: boolean;
}

export interface TreeViewProps<T> {
  /** The tree's accessible name. */
  readonly label: string;
  readonly roots: readonly T[];
  readonly getId: (item: T) => string;
  /** Plain text of the item, for typeahead. */
  readonly getText: (item: T) => string;
  readonly isExpandable: (item: T) => boolean;
  /** Keep this stable (memoize it) or pass new data through `roots`; it is called while rendering. */
  readonly getChildren: (item: T) => TreeChildren<T> | undefined;
  /** Called once when an expanded item's children are `undefined`. */
  readonly onLoadChildren?: (item: T) => void;
  readonly renderRow: (item: T, state: TreeRowState) => TreeRowContent;
  /** Controlled expansion (ids). Omit to let the tree keep it, starting from `defaultExpanded`. */
  readonly expanded?: ReadonlySet<string>;
  readonly defaultExpanded?: Iterable<string>;
  readonly onExpandedChange?: (expanded: ReadonlySet<string>) => void;
  readonly selectedId?: string | null;
  /** Click or Space. */
  readonly onSelect?: (item: T) => void;
  /** Double-click or Enter (Enter selects when there is no `onOpen`). */
  readonly onOpen?: (item: T) => void;
  readonly onContextMenu?: (item: T, at: Point) => void;
  /** Whether clicking an expandable row also toggles it (default true). */
  readonly toggleOnClick?: boolean;
  readonly rowHeight?: number;
  readonly indent?: number;
  readonly className?: string;
}

type Row<T> =
  | {
      readonly kind: "item";
      readonly id: string;
      readonly item: T;
      readonly depth: number;
      readonly parentId: string | null;
      readonly expandable: boolean;
      readonly expanded: boolean;
      readonly position: number;
      readonly setSize: number;
    }
  | {
      readonly kind: "status";
      readonly id: string;
      readonly parentId: string;
      readonly depth: number;
      readonly message: string | null;
    };

type ItemRow<T> = Extract<Row<T>, { kind: "item" }>;

interface FlattenInput<T> {
  readonly roots: readonly T[];
  readonly expanded: ReadonlySet<string>;
  readonly getId: (item: T) => string;
  readonly isExpandable: (item: T) => boolean;
  readonly getChildren: (item: T) => TreeChildren<T> | undefined;
}

/** The visible rows, in order. Iterative, so deep trees can't overflow the stack. */
function flattenTree<T>({ roots, expanded, getId, isExpandable, getChildren }: FlattenInput<T>): Row<T>[] {
  const rows: Row<T>[] = [];
  const stack: { items: readonly T[]; index: number; depth: number; parentId: string | null }[] = [
    { items: roots, index: 0, depth: 0, parentId: null },
  ];
  while (stack.length > 0) {
    const frame = stack.at(-1);
    if (!frame || frame.index >= frame.items.length) {
      stack.pop();
      continue;
    }
    const item = frame.items[frame.index] as T;
    frame.index += 1;
    const id = getId(item);
    const expandable = isExpandable(item);
    const open = expandable && expanded.has(id);
    rows.push({
      kind: "item",
      id,
      item,
      depth: frame.depth,
      parentId: frame.parentId,
      expandable,
      expanded: open,
      position: frame.index,
      setSize: frame.items.length,
    });
    if (!open) {
      continue;
    }
    const children = getChildren(item);
    if (children === undefined || ("status" in children && children.status === "loading")) {
      rows.push({ kind: "status", id: `${id}\u0000status`, parentId: id, depth: frame.depth + 1, message: null });
    } else if ("status" in children) {
      rows.push({ kind: "status", id: `${id}\u0000status`, parentId: id, depth: frame.depth + 1, message: children.message });
    } else if (children.length > 0) {
      stack.push({ items: children, index: 0, depth: frame.depth + 1, parentId: id });
    }
  }
  return rows;
}

/** The index range of `index`'s descendants: rows after it that are deeper. */
function descendantsEnd<T>(rows: readonly Row<T>[], index: number): number {
  const depth = rows[index]?.depth ?? 0;
  let end = index + 1;
  while (end < rows.length && (rows[end]?.depth ?? 0) > depth) {
    end += 1;
  }
  return end;
}

export function TreeView<T>({
  label,
  roots,
  getId,
  getText,
  isExpandable,
  getChildren,
  onLoadChildren,
  renderRow,
  expanded: controlledExpanded,
  defaultExpanded,
  onExpandedChange,
  selectedId = null,
  onSelect,
  onOpen,
  onContextMenu,
  toggleOnClick = true,
  rowHeight = 28,
  indent = 24,
  className,
}: TreeViewProps<T>) {
  const domId = useId();
  const scroller = useRef<HTMLDivElement>(null);
  const [innerExpanded, setInnerExpanded] = useState<ReadonlySet<string>>(() => new Set(defaultExpanded));
  const expanded = controlledExpanded ?? innerExpanded;
  const [activeId, setActiveId] = useState<string | null>(selectedId);
  const typed = useRef({ text: "", at: 0 });

  // A new selection from outside (e.g. "reveal in tree") becomes the active row.
  const [seenSelected, setSeenSelected] = useState(selectedId);
  if (seenSelected !== selectedId) {
    setSeenSelected(selectedId);
    if (selectedId !== null) {
      setActiveId(selectedId);
    }
  }

  const rows = useMemo(
    () => flattenTree({ roots, expanded, getId, isExpandable, getChildren }),
    [roots, expanded, getId, isExpandable, getChildren],
  );
  const indexById = useMemo(() => new Map(rows.map((row, index) => [row.id, index])), [rows]);
  const itemIndexes = useMemo(
    () => rows.flatMap((row, index) => (row.kind === "item" ? [index] : [])),
    [rows],
  );

  const firstItemId = itemIndexes[0] === undefined ? null : (rows[itemIndexes[0]]?.id ?? null);
  const active =
    activeId !== null && rows[indexById.get(activeId) ?? -1]?.kind === "item"
      ? activeId
      : selectedId !== null && indexById.has(selectedId)
        ? selectedId
        : firstItemId;
  const activeIndex = active === null ? -1 : (indexById.get(active) ?? -1);

  // Ask for children that expanded items need, once each until they arrive.
  const requested = useRef(new Set<string>());
  useEffect(() => {
    if (!onLoadChildren) {
      return;
    }
    for (const row of rows) {
      if (row.kind !== "item" || !row.expanded) {
        continue;
      }
      if (getChildren(row.item) === undefined) {
        if (!requested.current.has(row.id)) {
          requested.current.add(row.id);
          onLoadChildren(row.item);
        }
      } else {
        requested.current.delete(row.id);
      }
    }
  }, [rows, getChildren, onLoadChildren]);

  // TanStack Virtual hands back fresh functions each render by design; the app doesn't use the React
  // Compiler, so its "can't memoize" notice doesn't apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });

  useEffect(() => {
    if (selectedId === null) {
      return;
    }
    const index = indexById.get(selectedId);
    if (index !== undefined) {
      virtualizer.scrollToIndex(index, { align: "auto" });
    }
    // Only when the selection changes, not on every row change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);

  const setExpanded = (next: ReadonlySet<string>) => {
    if (controlledExpanded === undefined) {
      setInnerExpanded(next);
    }
    onExpandedChange?.(next);
  };

  const toggle = (row: ItemRow<T>, open: boolean) => {
    if (!row.expandable || row.expanded === open) {
      return;
    }
    const next = new Set(expanded);
    if (open) {
      next.add(row.id);
    } else {
      next.delete(row.id);
      // Collapsing an ancestor of the active row makes the ancestor active.
      const index = indexById.get(row.id) ?? -1;
      if (activeIndex > index && activeIndex < descendantsEnd(rows, index)) {
        setActiveId(row.id);
      }
    }
    setExpanded(next);
  };

  const moveTo = (index: number) => {
    const row = rows[index];
    if (row?.kind === "item") {
      setActiveId(row.id);
      virtualizer.scrollToIndex(index, { align: "auto" });
    }
  };

  /** Moves by `delta` item rows (skipping status rows), clamped. */
  const moveBy = (delta: number) => {
    const position = itemIndexes.indexOf(activeIndex);
    const next = itemIndexes[Math.max(0, Math.min(itemIndexes.length - 1, position + delta))];
    if (next !== undefined) {
      moveTo(next);
    }
  };

  const typeahead = (key: string) => {
    const now = Date.now();
    const text = (now - typed.current.at < 600 ? typed.current.text : "") + key.toLowerCase();
    typed.current = { text, at: now };
    const position = Math.max(0, itemIndexes.indexOf(activeIndex));
    const start = text.length === 1 ? position + 1 : position;
    const order = [...itemIndexes.slice(start), ...itemIndexes.slice(0, start)];
    const match = order.find((index) => {
      const row = rows[index];
      return row?.kind === "item" && getText(row.item).toLowerCase().startsWith(text);
    });
    if (match !== undefined) {
      moveTo(match);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const row = rows[activeIndex];
    if (row?.kind !== "item") {
      return;
    }
    if (isContextMenuKey(event)) {
      if (onContextMenu) {
        event.preventDefault();
        const element = document.getElementById(`${domId}-${activeIndex}`);
        onContextMenu(row.item, element ? pointBelow(element) : { x: 0, y: 0 });
      }
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }
    const page = Math.max(1, Math.floor((scroller.current?.clientHeight ?? rowHeight * 10) / rowHeight) - 1);
    switch (event.key) {
      case "ArrowDown":
        moveBy(1);
        break;
      case "ArrowUp":
        moveBy(-1);
        break;
      case "PageDown":
        moveBy(page);
        break;
      case "PageUp":
        moveBy(-page);
        break;
      case "Home":
        moveBy(-itemIndexes.length);
        break;
      case "End":
        moveBy(itemIndexes.length);
        break;
      case "ArrowRight":
        if (row.expandable && !row.expanded) {
          toggle(row, true);
        } else if (row.expanded && rows[activeIndex + 1]?.kind === "item") {
          moveTo(activeIndex + 1);
        }
        break;
      case "ArrowLeft":
        if (row.expanded) {
          toggle(row, false);
        } else if (row.parentId !== null) {
          moveTo(indexById.get(row.parentId) ?? activeIndex);
        }
        break;
      case "Enter":
        (onOpen ?? onSelect)?.(row.item);
        break;
      case " ":
        onSelect?.(row.item);
        break;
      case "*": {
        const next = new Set(expanded);
        for (const sibling of rows) {
          if (sibling.kind === "item" && sibling.parentId === row.parentId && sibling.expandable) {
            next.add(sibling.id);
          }
        }
        setExpanded(next);
        break;
      }
      default:
        // Letters, digits, and name punctuation jump by name; other keys (`/`, `]`) stay shortcuts.
        if (/^[\p{L}\p{N}_.$-]$/u.test(event.key)) {
          typeahead(event.key);
          break;
        }
        return;
    }
    event.preventDefault();
  };

  return (
    <div
      ref={scroller}
      role="tree"
      aria-label={label}
      aria-activedescendant={activeIndex >= 0 ? `${domId}-${activeIndex}` : undefined}
      tabIndex={0}
      className={cx("tree", className)}
      onKeyDown={onKeyDown}
    >
      <div className="tree-spacer" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtual) => {
          const row = rows[virtual.index];
          if (!row) {
            return null;
          }
          const style = { height: rowHeight, transform: `translateY(${virtual.start}px)` };
          const padding = { paddingLeft: 12 + row.depth * indent };
          if (row.kind === "status") {
            return (
              <div
                key={row.id}
                role="treeitem"
                aria-level={row.depth + 1}
                aria-disabled="true"
                aria-selected={false}
                className={cx("tree-row", "tree-row-status", row.message !== null && "tree-row-error")}
                style={style}
              >
                <div className="tree-row-inner" style={padding}>
                  <span className="tree-twisty" />
                  <span className="tree-label">{row.message ?? "Loading…"}</span>
                </div>
              </div>
            );
          }
          const selected = row.id === selectedId;
          const isActive = virtual.index === activeIndex;
          const content = renderRow(row.item, { depth: row.depth, expanded: row.expanded, selected, active: isActive });
          return (
            <div
              key={row.id}
              id={`${domId}-${virtual.index}`}
              role="treeitem"
              aria-level={row.depth + 1}
              aria-setsize={row.setSize}
              aria-posinset={row.position}
              aria-expanded={row.expandable ? row.expanded : undefined}
              aria-selected={selected}
              aria-busy={row.expanded && rows[virtual.index + 1]?.kind === "status" ? true : undefined}
              data-active={isActive || undefined}
              title={content.title}
              className={cx("tree-row", selected && "tree-row-selected", content.dimmed && "tree-row-dimmed")}
              style={style}
              onClick={() => {
                setActiveId(row.id);
                if (toggleOnClick) {
                  toggle(row, !row.expanded);
                }
                onSelect?.(row.item);
              }}
              onDoubleClick={() => {
                onOpen?.(row.item);
              }}
              onContextMenu={(event) => {
                if (onContextMenu) {
                  event.preventDefault();
                  setActiveId(row.id);
                  onContextMenu(row.item, { x: event.clientX, y: event.clientY });
                }
              }}
            >
              <div className="tree-row-inner" style={padding}>
                <span
                  className="tree-twisty"
                  onClick={(event) => {
                    if (row.expandable && !toggleOnClick) {
                      event.stopPropagation();
                      setActiveId(row.id);
                      toggle(row, !row.expanded);
                    }
                  }}
                >
                  {row.expandable ? (
                    <Icon name="chevron-right" size={14} className={cx("tree-chevron", row.expanded && "tree-chevron-open")} />
                  ) : null}
                </span>
                {content.icon === undefined ? null : (
                  <span className="tree-icon" aria-hidden="true">
                    {content.icon}
                  </span>
                )}
                <span className="tree-label">{content.label}</span>
                {content.description === undefined ? null : <span className="tree-description">{content.description}</span>}
                {content.marks === undefined ? null : <span className="tree-marks">{content.marks}</span>}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
