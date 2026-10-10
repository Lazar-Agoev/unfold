/**
 * Menus in HTML, in one style (V-43): `Menu` (the keyboard-navigable list), `ContextMenu` (a Menu at
 * the pointer), `useContextMenu` (state for one), and `MenuButton` (a button that opens a Menu below
 * it). An item may hold a submenu, which opens to its right with its first item level with the row.
 *
 * Keyboard: ↑/↓ move (skipping disabled items and wrapping), Home/End jump, Enter/Space choose,
 * → opens a submenu and ← or Escape leaves it, a letter jumps to the next item starting with it,
 * Escape or Tab closes. Focus returns to where it was.
 */

import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode, type RefObject } from "react";
import { Icon, type IconName } from "../icons";
import { Button, type ButtonProps } from "./Button";
import { IconButton, type IconButtonProps } from "./IconButton";
import { cx, pointBelow, type Point } from "./internal/dom";
import { useFloatingPosition, type Align, type Anchor, type Side } from "./internal/floating";
import { Portal, useDismiss, useRestoreFocus } from "./internal/layers";
import { ShortcutText } from "./internal/shortcut";
import "./Menu.css";

interface MenuItemBase {
  readonly kind?: "item";
  readonly id: string;
  readonly label: string;
  readonly icon?: IconName;
  /** Shown on the right, e.g. `Mod+W`. Display only: the action registry owns the key. */
  readonly shortcut?: string;
  readonly disabled?: boolean;
  /** Destructive items (Delete…) are drawn in the danger color. */
  readonly danger?: boolean;
  /** Present for checkable items; renders `menuitemcheckbox` with a check in the icon slot. */
  readonly checked?: boolean;
  /** Content for the 16px trailing slot. */
  readonly trailing?: ReactNode;
}

/** A command, or a row that opens a submenu (Share ›, Move to folder ›). */
type MenuItem = MenuItemBase &
  ({ readonly onSelect: () => void; readonly submenu?: undefined } | { readonly submenu: readonly MenuEntry[]; readonly onSelect?: undefined });

interface MenuSeparator {
  readonly kind: "separator";
  readonly id: string;
}

/** A non-interactive section heading inside a menu. */
interface MenuHeading {
  readonly kind: "heading";
  readonly id: string;
  readonly label: string;
}

export type MenuEntry = MenuItem | MenuSeparator | MenuHeading;

function isItem(entry: MenuEntry): entry is MenuItem {
  return entry.kind === undefined || entry.kind === "item";
}

interface MenuProps {
  readonly entries: readonly MenuEntry[];
  /** The menu's accessible name. */
  readonly label: string;
  /** Called after a choice, on Escape/Tab, and by the layer that hosts the menu. */
  readonly onClose: () => void;
  /** Which item takes focus first. */
  readonly initialFocus?: "first" | "last";
  /** Fixed width in px (V-43: board 220, project 264, peek 208, share 196); otherwise it fits its items. */
  readonly width?: number;
  readonly className?: string;
}

interface MenuListProps extends Omit<MenuProps, "initialFocus"> {
  readonly initialFocus: "first" | "last" | null;
  /** For a submenu: ← and Escape return to the parent row. */
  readonly onBack?: () => void;
}

function enabledItems(container: HTMLElement | null): HTMLElement[] {
  return [...(container?.querySelectorAll<HTMLElement>(":scope > [data-menu-item]:not([aria-disabled='true'])") ?? [])];
}

interface OpenSubmenu {
  readonly id: string;
  readonly anchor: HTMLElement;
  /** Whether the submenu's first item takes focus (keyboard) or focus stays on the row (hover). */
  readonly focus: boolean;
}

function MenuList({ entries, label, onClose, initialFocus, width, className, onBack }: MenuListProps) {
  const list = useRef<HTMLDivElement>(null);
  const typed = useRef({ text: "", at: 0 });
  const [submenu, setSubmenu] = useState<OpenSubmenu | null>(null);

  useEffect(() => {
    if (initialFocus === null) {
      return;
    }
    const all = enabledItems(list.current);
    (initialFocus === "last" ? all.at(-1) : all[0])?.focus({ preventScroll: true });
  }, [initialFocus]);

  const hasIcons = entries.some((entry) => isItem(entry) && (entry.icon !== undefined || entry.checked !== undefined));
  const hasTrailing = entries.some((entry) => isItem(entry) && (entry.submenu !== undefined || entry.trailing !== undefined));

  const openSubmenu = (item: MenuItem, anchor: HTMLElement, focus: boolean) => {
    if (focus && submenu?.id === item.id) {
      // Already open from hover: its layer renders right after this list.
      enabledItems(list.current?.nextElementSibling?.querySelector<HTMLElement>("[role=menu]") ?? null)[0]?.focus();
      return;
    }
    setSubmenu({ id: item.id, anchor, focus });
  };

  const choose = (item: MenuItem, element: HTMLElement) => {
    if (item.disabled) {
      return;
    }
    if (item.submenu) {
      openSubmenu(item, element, true);
      return;
    }
    onClose();
    item.onSelect();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const all = enabledItems(list.current);
    const index = all.findIndex((element) => element === document.activeElement);
    const focusAt = (next: number) => {
      all[(next + all.length) % all.length]?.focus();
    };
    switch (event.key) {
      case "ArrowDown":
        focusAt(index + 1);
        break;
      case "ArrowUp":
        focusAt(index < 0 ? -1 : index - 1);
        break;
      case "Home":
        focusAt(0);
        break;
      case "End":
        focusAt(-1);
        break;
      case "ArrowLeft":
        onBack?.();
        break;
      case "Escape":
        (onBack ?? onClose)();
        break;
      case "Tab":
        onClose();
        break;
      default: {
        if (event.key.length !== 1 || event.metaKey || event.ctrlKey || event.altKey) {
          return;
        }
        // Typeahead: letters typed in quick succession spell an item's start.
        const now = Date.now();
        typed.current = { text: (now - typed.current.at < 600 ? typed.current.text : "") + event.key.toLowerCase(), at: now };
        const prefix = typed.current.text;
        const start = index + (prefix.length === 1 ? 1 : 0);
        const order = [...all.slice(start), ...all.slice(0, start)];
        order.find((element) => element.textContent.trim().toLowerCase().startsWith(prefix))?.focus();
        break;
      }
    }
    event.preventDefault();
    event.stopPropagation();
  };

  const openItem = submenu === null ? undefined : entries.find((entry): entry is MenuItem => isItem(entry) && entry.id === submenu.id);

  return (
    <>
      <div
        ref={list}
        role="menu"
        aria-label={label}
        className={cx("menu", className)}
        style={width === undefined ? undefined : { width }}
        onKeyDown={onKeyDown}
      >
        {entries.map((entry) => {
          if (entry.kind === "separator") {
            return <div key={entry.id} role="separator" className="menu-separator" />;
          }
          if (entry.kind === "heading") {
            return (
              <div key={entry.id} role="presentation" className={cx("menu-heading", hasIcons && "menu-heading-indented")}>
                {entry.label}
              </div>
            );
          }
          if (!isItem(entry)) {
            return null;
          }
          const checkable = entry.checked !== undefined;
          const parent = entry.submenu !== undefined;
          return (
            <div
              key={entry.id}
              role={checkable ? "menuitemcheckbox" : "menuitem"}
              aria-checked={checkable ? entry.checked : undefined}
              aria-disabled={entry.disabled ?? undefined}
              aria-haspopup={parent ? "menu" : undefined}
              aria-expanded={parent ? submenu?.id === entry.id : undefined}
              tabIndex={-1}
              data-menu-item=""
              className={cx("menu-item", entry.danger && "menu-item-danger")}
              onClick={(event) => {
                choose(entry, event.currentTarget);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " " || (event.key === "ArrowRight" && parent)) {
                  event.preventDefault();
                  event.stopPropagation();
                  choose(entry, event.currentTarget);
                }
              }}
              onPointerMove={(event) => {
                if (entry.disabled) {
                  return;
                }
                if (document.activeElement !== event.currentTarget) {
                  event.currentTarget.focus({ preventScroll: true });
                }
                if (parent && submenu?.id !== entry.id) {
                  setSubmenu({ id: entry.id, anchor: event.currentTarget, focus: false });
                } else if (!parent && submenu !== null) {
                  setSubmenu(null);
                }
              }}
            >
              {hasIcons ? (
                <span className="menu-item-icon" aria-hidden="true">
                  {checkable ? entry.checked ? <Icon name="check" /> : null : entry.icon ? <Icon name={entry.icon} /> : null}
                </span>
              ) : null}
              <span className="menu-item-label">{entry.label}</span>
              {entry.shortcut ? <ShortcutText shortcut={entry.shortcut} className="menu-item-shortcut" /> : null}
              {hasTrailing ? (
                <span className="menu-item-trailing" aria-hidden={parent || undefined}>
                  {parent ? <Icon name="chevron-right" size={14} /> : entry.trailing}
                </span>
              ) : null}
            </div>
          );
        })}
      </div>
      {submenu !== null && openItem?.submenu ? (
        <Submenu
          key={submenu.id}
          anchor={submenu.anchor}
          entries={openItem.submenu}
          label={openItem.label}
          focus={submenu.focus}
          onClose={onClose}
          onBack={() => {
            setSubmenu(null);
            submenu.anchor.focus();
          }}
        />
      ) : null}
    </>
  );
}

interface SubmenuProps {
  readonly anchor: HTMLElement;
  readonly entries: readonly MenuEntry[];
  readonly label: string;
  readonly focus: boolean;
  readonly onClose: () => void;
  readonly onBack: () => void;
}

/** The menu's border and padding: a row sits this far inside the menu's edge. */
const MENU_INSET = 5;

/**
 * A submenu, 2px right of its menu with its first item level with the row. It renders inside the
 * parent's layer, so presses in it aren't outside presses.
 */
function Submenu({ anchor, entries, label, focus, onClose, onBack }: SubmenuProps) {
  const position = useFloatingPosition<HTMLDivElement>(anchor, { side: "right", align: "start", shift: -MENU_INSET }, true, MENU_INSET + 2);
  return (
    <div ref={position} className="menu-layer">
      <MenuList entries={entries} label={label} onClose={onClose} onBack={onBack} initialFocus={focus ? "first" : null} />
    </div>
  );
}

/** The menu list itself. Hosts (ContextMenu, MenuButton) position it and handle outside presses. */
function Menu({ initialFocus = "first", ...props }: MenuProps) {
  return <MenuList {...props} initialFocus={initialFocus} />;
}

interface FloatingMenuProps extends Omit<MenuProps, "initialFocus"> {
  readonly anchor: Anchor;
  readonly side: Side;
  readonly align: Align;
  readonly initialFocus: "first" | "last";
  /** Presses on this element (the trigger) don't count as outside. */
  readonly trigger?: RefObject<HTMLElement | null>;
}

function FloatingMenu({ anchor, side, align, trigger, ...menu }: FloatingMenuProps) {
  const layer = useFloatingPosition<HTMLDivElement>(anchor, { side, align }, true, 4);
  useDismiss(layer, true, menu.onClose, trigger);
  useRestoreFocus(true);
  return (
    <Portal>
      <div ref={layer} className="menu-layer">
        <Menu {...menu} />
      </div>
    </Portal>
  );
}

interface ContextMenuProps {
  /** Where the menu opens: the pointer, or below an element for keyboard-opened menus. */
  readonly at: Point;
  readonly entries: readonly MenuEntry[];
  readonly label: string;
  readonly onClose: () => void;
  readonly width?: number;
}

/** A Menu at a point, in a portal, closing on a press outside or when the window loses focus. */
function ContextMenu({ at, ...menu }: ContextMenuProps) {
  return <FloatingMenu anchor={at} side="bottom" align="start" initialFocus="first" {...menu} />;
}

interface ContextMenuState {
  readonly at: Point;
  readonly entries: readonly MenuEntry[];
}

export interface ContextMenuController {
  /**
   * Opens the menu at a mouse event's position (and prevents the browser's menu), at a point, or
   * below an element (for Shift+F10 and the Menu key).
   */
  readonly open: (at: MouseEvent | Point | Element, entries: readonly MenuEntry[]) => void;
  readonly close: () => void;
  /** Render this somewhere in the component; it is null while closed. */
  readonly element: ReactNode;
}

/** State for one context menu. Pair with `onContextMenu` callbacks from rows, trees, and lists. */
export function useContextMenu(label: string, width?: number): ContextMenuController {
  const [state, setState] = useState<ContextMenuState | null>(null);
  const close = () => {
    setState(null);
  };
  return {
    open: (at, entries) => {
      let point: Point;
      if (at instanceof Element) {
        point = pointBelow(at);
      } else if ("clientX" in at) {
        at.preventDefault();
        point = { x: at.clientX, y: at.clientY };
      } else {
        point = at;
      }
      setState({ at: point, entries });
    },
    close,
    element: state ? (
      <ContextMenu at={state.at} entries={state.entries} label={label} onClose={close} {...(width === undefined ? {} : { width })} />
    ) : null,
  };
}

interface MenuButtonBase {
  readonly entries: readonly MenuEntry[];
  /** The menu's accessible name; defaults to the button's label. */
  readonly menuLabel?: string;
  readonly side?: Side;
  readonly align?: Align;
  readonly width?: number;
}

export type MenuButtonProps = MenuButtonBase &
  (
    | ({ readonly variant: "icon" } & Omit<IconButtonProps, "onClick" | "pressed">)
    | ({ readonly variant: "text"; readonly label: string } & Omit<ButtonProps, "onClick" | "variant">)
  );

/** A button that opens a menu below it (the board menu `⋯`, the sort menu). ↓ also opens it. */
export function MenuButton(props: MenuButtonProps) {
  const { entries, menuLabel, side = "bottom", align = "start", width } = props;
  const [open, setOpen] = useState<{ anchor: HTMLElement; focus: "first" | "last" } | null>(null);
  const button = useRef<HTMLButtonElement>(null);

  const common = {
    ref: button,
    "aria-haspopup": "menu" as const,
    "aria-expanded": open !== null,
    onClick: (event: MouseEvent<HTMLButtonElement>) => {
      const anchor = event.currentTarget;
      setOpen((current) => (current ? null : { anchor, focus: "first" }));
    },
    onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        setOpen({ anchor: event.currentTarget, focus: event.key === "ArrowDown" ? "first" : "last" });
      }
    },
  };

  let trigger: ReactNode;
  if (props.variant === "icon") {
    const { entries: _entries, menuLabel: _menuLabel, side: _side, align: _align, width: _width, variant: _variant, ...rest } = props;
    trigger = <IconButton {...rest} {...common} />;
  } else {
    const { entries: _entries, menuLabel: _menuLabel, side: _side, align: _align, width: _width, variant: _variant, label: text, ...rest } = props;
    trigger = (
      <Button {...rest} {...common} variant="ghost">
        {text}
      </Button>
    );
  }

  return (
    <>
      {trigger}
      {open ? (
        <FloatingMenu
          anchor={open.anchor}
          side={side}
          align={align}
          trigger={button}
          initialFocus={open.focus}
          entries={entries}
          label={menuLabel ?? props.label}
          {...(width === undefined ? {} : { width })}
          onClose={() => {
            setOpen(null);
          }}
        />
      ) : null}
    </>
  );
}
