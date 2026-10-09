# Kit API (U1a)

What `theme/`, `icons/`, `components/`, and `lib/` export, as built by U1a, for U1c and U2–U5. Import components
only from `components/index.ts`, icons from `icons/index.ts`, and helpers from `lib/<file>.ts`. Every component also
takes the standard HTML attributes of its root element where noted ("+ HTML"), so `className`, `id`, `role`, `aria-*`,
and `data-*` pass through.

## theme/

| Export | Signature |
| --- | --- |
| `applyThemeOverride` | `(theme: ThemeOverride, root?: HTMLElement) => void` |
| `ThemeOverride` | `"light" \| "dark" \| null` |

Tokens are in `theme/tokens.css` (UI plan §5). Type sizes pair with a leading: `font-size: var(--text-13);
line-height: var(--leading-20)`. Sizes: `--text-11 | 12 | 13 | 15 | 16 | 17 | 32`; leadings: `--leading-14 | 16 | 20 |
22 | 24 | 40`. Code text never ligates (`code, pre` in `base.css`; apply the same `font-feature-settings` to other code
elements).

## icons/

| Export | Signature |
| --- | --- |
| `Icon` | `{ name: IconName; size?: 16 \| 14 \| 12 \| 10; className?: string }` (default 16; `aria-hidden`, `currentColor`, `data-icon={name}`) |
| `IconName` | union of `ICON_NAMES` |
| `ICON_NAMES` | `home, chevron-left, chevron-right, chevron-down, chevron-right-small, chevron-down-small, dots, plus, page, entrance, group, link, search, clock, drafts, folder, board, board-draft, branch, check, gear, arrange, external-link, reveal, pin, close, add-circle, warning-circle, warning-triangle, info, shield, publish, move-back, copy, activity, edit, layers, minus, back, forward` |
| `StateGlyph` | `{ glyph: "ring" \| "paused" \| "not-compiled" \| "failed" \| "resting"; variant: "row" \| "line"; strokeWidth?: number; className?: string }` (used by `StateIndicator` and `ProgressRing`; screens use those) |

Each glyph keeps the viewBox it was drawn in (16 for the Home set, 24 for the board artboards' icons, 10 for the small
chevrons), so a 24-box icon at 16px draws a 1px line, as in Paper. `chevron-*` are the board's 24-box chevrons (breadcrumb,
dock, menus); `chevron-*-small` are the sidebar's 10-box disclosure chevrons (render at `size={10}`). `back`/`forward`
are arrows for the navigation commands; the dock draws `chevron-left`/`chevron-right`. `dots` is the filled `⋯`.

## components/

### Buttons

| Export | Props |
| --- | --- |
| `Button` | + HTML button. `variant: "primary" \| "secondary" \| "ghost" \| "danger"` (required); `size?: "md" \| "sm"` (30 / 28px; ghost is always 28); `icon?: IconName` (14px, leading); `trailingIcon?: IconName` (14px, Share's chevron); `shortcut?: string` (`Mod+Shift+B` form, shown in the platform's style, `--color-on-accent-soft` on primary); `busy?: boolean` (ring in place of the icon, `aria-busy`); `ref?`. `type` defaults to `"button"` |
| `TextButton` | + HTML button. `children`; `weight?: 400 \| 500` (default 500); `tone?: "accent" \| "muted"` (default accent); takes the surrounding font size |
| `IconButton` | + HTML button. `icon: IconName`; `label: string` (accessible name and tooltip); `size?: 28 \| 24 \| 20` (default 28; icon 16 in each); `pressed?: boolean` (`aria-pressed` + `--color-pressed`; an open `MenuButton` looks the same); `shortcut?: string` (in the tooltip); `tooltipSide?: Side`; `ref?` |
| `KeyChip` | `shortcut?: string` or `glyph?: string`; `variant: "filled" \| "outlined" \| "keycap"`; `selected?: boolean` (keycap border `--color-selection-line`); `className?`. A `keycap` with a `shortcut` draws one cap per key; `glyph=">"` is set in the code face |

### Menus and floating layers

| Export | Props |
| --- | --- |
| `MenuEntry` | `MenuItem \| { kind: "separator"; id } \| { kind: "heading"; id; label }` |
| `MenuItem` (via `MenuEntry`) | `{ kind?: "item"; id; label; icon?: IconName; shortcut?: string; disabled?; danger?; checked?: boolean; trailing?: ReactNode }` and either `onSelect: () => void` or `submenu: MenuEntry[]` |
| `Menu` | `entries; label; onClose; initialFocus?: "first" \| "last"; width?: number; className?` |
| `ContextMenu` | `at: Point; entries; label; onClose; width?: number` |
| `useContextMenu` | `(label: string, width?: number) => { open(at: MouseEvent \| Point \| Element, entries), close(), element: ReactNode }` |
| `MenuButton` | `entries; menuLabel?; side?: Side; align?: "start" \| "center" \| "end"; width?: number` and either `variant: "icon"` + `IconButton` props (minus `onClick`, `pressed`) or `variant: "text"; label` + `Button` props (minus `onClick`, `variant`; drawn as ghost). Opens 4px below; pass `align="end"` for the right-aligned board and project menus |
| `Popover` | `open; onClose; anchor: HTMLElement \| null; label; side?: Side; align?; offset?: number` (gap, default 4); `width?: number`; `placement?: "anchor" \| "fixed"` (`anchor` follows the anchor on scroll; `fixed` stays where it opened); `className?; children`. Takes focus, closes on Escape and outside presses |
| `Tooltip` | `content: ReactNode; shortcut?; side?; align?; describe?: boolean` (default true: also the trigger's description); `disabled?; children` (one element). 500 ms delay; popover style, 12/16 |
| `HoverCard` | `content; side?; align?; openDelay?: number` (default 400); `disabled?; className?; children`. Pointer may move into it |
| `Dialog` | `open; onClose; title: string; description?: ReactNode; width: 400 \| 520 \| 880; height?: number; placement: "window" \| "canvas"; footer?: ReactNode; initialFocus?: RefObject<HTMLElement \| null>; role?: "dialog" \| "alertdialog"; className?; children?`. `canvas` puts the scrim over `#content-area` and centers on it. Width 880 adds a Close button and ruled header and footer, and its body is a flex row with no padding |
| `ConfirmDialog` | `open; title; message: ReactNode; confirmLabel; cancelLabel?; danger?: boolean; placement: "window" \| "canvas"; onConfirm; onCancel` (400 wide) |
| `PromptDialog` | `open; title; description?; label; defaultValue?; confirmLabel; cancelLabel?; validate?: (value) => string \| null; placement: "window" \| "canvas"; onSubmit(value); onCancel` (400 wide) |
| `Sheet` | + HTML aside. `open; label; width?: number` (default `--peek-width` 640); `pinned?: boolean; onClose; children`. Mount as the last child of `#content-area` (a positioned flex row): unpinned it floats over the content (z `--z-sheet`), pinned it takes its place and the content narrows. Escape inside closes it; not modal |
| `Toaster` | no props; render once |
| `showToast` | `(options: ToastOptions) => string` (the id) |
| `dismissToast` | `(id: string) => void` |
| `ToastOptions` | `{ title: string; body?: string; leading?: ReactNode; action?: { label; run }; duration?: number \| null; tone?: "info" \| "danger"; id?: string }`. Defaults: 5 s, 8 s for danger, `null` stays. Same `id` replaces in place. At most 4, newest at the bottom; danger toasts get a warning icon unless `leading` is given |

### Controls

| Export | Props |
| --- | --- |
| `SegmentedControl` | `options: { value: V; label: string; count?: number }[]; value: V; onChange(value: V); label: string; variant: "lens" \| "mini" \| "switch"; className?` |
| `Switch` | + HTML button. `checked: boolean; onChange(checked: boolean); label: string` (accessible name); `disabled?` |
| `Checkbox` | + HTML input. `checked: boolean \| "mixed"; onChange(checked: boolean)` (mixed → `true`); `label: string` (accessible name); `disabled?` |
| `RadioGroup` | `options: { value: V; label: string }[]; value: V; onChange(value: V); label: string; className?` |
| `TextField` | + HTML input. `label: string; hideLabel?: boolean; hint?: string; error?: string \| null; ref?` |
| `SearchField` | + HTML input (minus `value`, `onChange`, `type`). `value: string; onChange(value: string); label: string; focusKey?: string; ref?`. Escape clears, then blurs |
| `ResizeHandle` | `edge: "left" \| "right" \| "top" \| "bottom"; size: number; bounds: SizeBounds; onResize(size); onResizeEnd?(size); label: string; controls?: string; className?`. Absolutely placed on the pane's edge: the pane must be `position: relative` |
| `SizeBounds` | `{ min: number; max: number; initial: number }` |

### Rows, labels, and marks

| Export | Props |
| --- | --- |
| `ListRow` | + HTML div (minus the three handlers). `leading?: ReactNode` (16px slot, accent when selected); `label: ReactNode`; `trailing?: ReactNode` (16px slot, grows when given content); `depth?: number; indent?: number` (default 16 px per level); `selected?; muted?; onClick?(event)` (also Enter on the focused row); `onDoubleClick?(event); onContextMenu?(event); menu?: ReactNode` (after the trailing slot, visible on hover, focus, selection, and while its menu is open). Give it its `role` and `tabIndex`. Set `--list-row-inline` / `--list-row-gap` (both 8px) to 6px for the Home sidebar |
| `SectionLabel` | + HTML div. `children; trailing?: ReactNode; as?: "h2" \| "h3"` (default h2). Row padding 0 8; an `IconButton` in `trailing` sits 4px from the edge |
| `Monogram` | `name: string; size: 14 \| 20; tone?: "default" \| "selected" \| "workspace"; className?` |
| `StateIndicator` | `state: ProjectState` (contract §4.1, structurally; the generated type fits); `variant: "row" \| "line"; className?`. Row: current is an empty slot; problems and unavailable explain themselves in a tooltip |
| `ProgressRing` | `size: 14 \| 12; strokeWidth?: number` (default 1.75 / 2). Rotates; takes the text color for its arc |
| `KindGlyph` | `kind: "function" \| "entrance" \| "group" \| "link"; selected?: boolean` |
| `Badge` | + HTML span. `icon?: IconName; children` |
| `EmptyState` | + HTML div (minus `title`). `title: string; body?: ReactNode; action?: ReactNode` |

### Lists

| Export | Props |
| --- | --- |
| `VirtualList` | `label; groups: ListGroup<T>[]; getId(item); renderItem(item, state: ListItemState); getText?(item); rowHeight?: number` (default 28; Add uses 32); `getItemHeight?(item)` (overrides `rowHeight`); `headerHeight?: number` (default 24); `selectedId?; onActivate?(item); onActiveChange?(item); onContextMenu?(item, at: Point); empty?: ReactNode; className?; ref?`. Sticky headers paint `--vlist-background` (default `--color-surface`) |
| `ListGroup<T>` | `{ id; label: ReactNode; count?: ReactNode; items: T[] }` |
| `ListItemState` | `{ active: boolean; selected: boolean }` |
| `TreeView` | V1 API: `label; roots; getId; getText; isExpandable; getChildren(item): TreeChildren<T> \| undefined; onLoadChildren?; renderRow(item, state: TreeRowState): TreeRowContent; expanded?; defaultExpanded?; onExpandedChange?; selectedId?; onSelect?; onOpen?; onContextMenu?(item, at: Point); toggleOnClick?; rowHeight?` (default 28); `indent?` (default 24); `className?` |
| `TreeChildren<T>` | `T[] \| { status: "loading" } \| { status: "error"; message: string }` |
| `TreeRowContent` | `{ label: ReactNode; icon?: ReactNode; description?: ReactNode; marks?: ReactNode; dimmed?: boolean; title?: string }` |
| `TreeRowState` | `{ depth; expanded; selected; active }` |

### Shared types

| Export | Definition |
| --- | --- |
| `Point` | `{ x: number; y: number }` |
| `Side` | `"top" \| "bottom" \| "left" \| "right"` |

## lib/

| File | Export | Signature |
| --- | --- | --- |
| `format.ts` | `formatCount` | `(count: number, max?: number) => string` — `1,284`; `500+` at the cap |
| | `plural` | `(count: number, one: string, many?: string) => string` — `1 file`, `1,058 files` |
| | `languageList` | `(languages: readonly Language[]) => string` — `TypeScript + Python` |
| | `displayPath` | `(path: string) => string` — home folder as `~` (`/Users/x`, `/home/x`, `C:\Users\x`) |
| | `relativeTime` | `(atMs: number, nowMs?: number) => string` — "just now" … "Mar 4, 2024" |
| `entrances.ts` | `methodColumn` | `(row: Pick<EntranceRow, "method" \| "kind">) => string` |
| | `triggerText` | `(row: Pick<EntranceRow, "label" \| "trigger" \| "triggerStatus" \| "kind">) => string` |
| `files.ts` | `openInEditor` | `(project: ProjectId, path: string, line?: number \| null) => Promise<void>` — `open_in_editor`; failures toast |
| | `revealInFinder` | `(project: ProjectId, path: string) => Promise<void>` — `reveal_in_finder` (`""` = the project folder); failures toast |

`lib/keys.ts` (`pageKey`, `cardRefKey`, plan §2) is not part of U1a.
