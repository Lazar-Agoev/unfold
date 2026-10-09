/**
 * Menus in HTML: `Menu` (the keyboard-navigable list), `ContextMenu` (a Menu at the pointer),
 * `useContextMenu` (state for one), and `MenuButton` (a button that opens a Menu below it).
 *
 * Keyboard: ↑/↓ move (skipping disabled items and wrapping), Home/End jump, Enter/Space choose,
 * a letter jumps to the next item starting with it, Escape or Tab closes. Focus returns to where it was.
 */

import { Check, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode, type RefObject } from "react";
import { Button, type ButtonProps } from "./Button";
import { IconButton, type IconButtonProps } from "./IconButton";
import { cx, pointBelow, type Point } from "./internal/dom";
import { useFloatingPosition, type Align, type Anchor, type Side } from "./internal/floating";
import { Portal, useDismiss, useRestoreFocus } from "./internal/layers";
import { Kbd } from "./Kbd";
import "./Menu.css";

export interface MenuItem {
  readonly kind?: "item";
  readonly id: string;
  readonly label: string;
  readonly icon?: LucideIcon;
  /** Shown on the right, e.g. `Mod+W`. Display only: the action registry owns the key. */
  readonly shortcut?: string;
  readonly disabled?: boolean;
  /** Destructive items (Delete) are drawn in the danger color. */
  readonly danger?: boolean;
  /** Present for checkable items; renders `menuitemcheckbox`. */
  readonly checked?: boolean;
  readonly onSelect: () => void;
}

export interface MenuSeparator {
  readonly kind: "separator";
  readonly id: string;
}

/** A non-interactive section heading inside a menu. */
export interface MenuHeading {
  readonly kind: "heading";
  readonly id: string;
  readonly label: string;
}

export type MenuEntry = MenuItem | MenuSeparator | MenuHeading;

function isItem(entry: MenuEntry): entry is MenuItem {
  return entry.kind === undefined || entry.kind === "item";
}

export interface MenuProps {
  readonly entries: readonly MenuEntry[];
  /** The menu's accessible name. */
  readonly label: string;
  /** Called after a choice, on Escape/Tab, and by the layer that hosts the menu. */
  readonly onClose: () => void;
  /** Which item takes focus first. */
  readonly initialFocus?: "first" | "last";
  readonly className?: string;
}

function enabledItems(list: HTMLElement | null): HTMLElement[] {
  return [...(list?.querySelectorAll<HTMLElement>("[data-menu-item]:not([aria-disabled='true'])") ?? [])];
}

/** The menu list itself. Hosts (ContextMenu, MenuButton) position it and handle outside presses. */
export function Menu({ entries, label, onClose, initialFocus = "first", className }: MenuProps) {
  const list = useRef<HTMLDivElement>(null);
  const typed = useRef({ text: "", at: 0 });

  useEffect(() => {
    const all = enabledItems(list.current);
    (initialFocus === "last" ? all.at(-1) : all[0])?.focus({ preventScroll: true });
  }, [initialFocus]);

  const choose = (item: MenuItem) => {
    if (item.disabled) {
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
      case "Escape":
        onClose();
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
        const order = [...all.slice(index + (prefix.length === 1 ? 1 : 0)), ...all.slice(0, index + (prefix.length === 1 ? 1 : 0))];
        order.find((element) => element.textContent.trim().toLowerCase().startsWith(prefix))?.focus();
        break;
      }
    }
    event.preventDefault();
    event.stopPropagation();
  };

  return (
    <div ref={list} role="menu" aria-label={label} className={cx("menu", className)} onKeyDown={onKeyDown}>
      {entries.map((entry) => {
        if (entry.kind === "separator") {
          return <div key={entry.id} role="separator" className="menu-separator" />;
        }
        if (entry.kind === "heading") {
          return (
            <div key={entry.id} role="presentation" className="menu-heading">
              {entry.label}
            </div>
          );
        }
        if (!isItem(entry)) {
          return null;
        }
        const Icon = entry.icon;
        const checkable = entry.checked !== undefined;
        return (
          <div
            key={entry.id}
            role={checkable ? "menuitemcheckbox" : "menuitem"}
            aria-checked={checkable ? entry.checked : undefined}
            aria-disabled={entry.disabled ?? undefined}
            tabIndex={-1}
            data-menu-item=""
            className={cx("menu-item", entry.danger && "menu-item-danger")}
            onClick={() => {
              choose(entry);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                event.stopPropagation();
                choose(entry);
              }
            }}
            onPointerMove={(event) => {
              if (!entry.disabled && document.activeElement !== event.currentTarget) {
                event.currentTarget.focus({ preventScroll: true });
              }
            }}
          >
            <span className="menu-item-icon" aria-hidden="true">
              {checkable ? entry.checked ? <Check size={14} /> : null : Icon ? <Icon size={14} /> : null}
            </span>
            <span className="menu-item-label">{entry.label}</span>
            {entry.shortcut ? <Kbd shortcut={entry.shortcut} tone="plain" className="menu-item-kbd" /> : null}
          </div>
        );
      })}
    </div>
  );
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
  const layer = useFloatingPosition<HTMLDivElement>(anchor, { side, align }, true, 2);
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

export interface ContextMenuProps {
  /** Where the menu opens: the pointer, or below an element for keyboard-opened menus. */
  readonly at: Point;
  readonly entries: readonly MenuEntry[];
  readonly label: string;
  readonly onClose: () => void;
}

/** A Menu at a point, in a portal, closing on a press outside or when the window loses focus. */
export function ContextMenu({ at, entries, label, onClose }: ContextMenuProps) {
  return <FloatingMenu anchor={at} side="bottom" align="start" initialFocus="first" entries={entries} label={label} onClose={onClose} />;
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

/** State for one context menu. Pair with `onContextMenu` callbacks from TreeView, Tabs, and lists. */
export function useContextMenu(label: string): ContextMenuController {
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
    element: state ? <ContextMenu at={state.at} entries={state.entries} label={label} onClose={close} /> : null,
  };
}

interface MenuButtonBase {
  readonly entries: readonly MenuEntry[];
  /** The menu's accessible name; defaults to the button's label. */
  readonly menuLabel?: string;
  readonly side?: Side;
  readonly align?: Align;
}

export type MenuButtonProps = MenuButtonBase &
  (
    | ({ readonly variant: "icon" } & Omit<IconButtonProps, "onClick" | "pressed">)
    | ({ readonly variant: "text"; readonly label: string } & Omit<ButtonProps, "onClick" | "variant">)
  );

/** A button that opens a menu below it (the board menu `⋯`, the folder switcher). ↓ also opens it. */
export function MenuButton(props: MenuButtonProps) {
  const { entries, menuLabel, side = "bottom", align = "start" } = props;
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
    const { entries: _entries, menuLabel: _menuLabel, side: _side, align: _align, variant: _variant, ...rest } = props;
    trigger = <IconButton {...rest} {...common} />;
  } else {
    const { entries: _entries, menuLabel: _menuLabel, side: _side, align: _align, variant: _variant, label: text, ...rest } = props;
    trigger = (
      <Button {...rest} {...common} variant="quiet">
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
          onClose={() => {
            setOpen(null);
          }}
        />
      ) : null}
    </>
  );
}
