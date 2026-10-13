/**
 * Hover layers in the popover style. A Tooltip names a control or explains
 * a mark (the problems dot's count, the state line's notes); a HoverCard shows richer, delayed detail,
 * such as a call link's signature and location. Both open on hover after a delay and on keyboard
 * focus, close on Escape, and wrap their trigger without adding a box to the layout.
 */

import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { cx } from "./internal/dom";
import { useFloatingPosition, type Align, type Side } from "./internal/floating";
import { Portal } from "./internal/layers";
import "./Tooltip.css";

interface HoverLayerOptions {
  readonly openDelay: number;
  readonly closeDelay: number;
  /** Whether the pointer may move into the layer (hover cards) without it closing. */
  readonly interactive: boolean;
  readonly disabled: boolean;
}

/** When a tooltip last closed: moving straight to another control shows its tooltip at once. */
let lastClosedAt = 0;
const WARM_WINDOW_MS = 400;

function focusIsVisible(element: Element): boolean {
  try {
    return element.matches(":focus-visible");
  } catch {
    return true;
  }
}

function useHoverLayer({ openDelay, closeDelay, interactive, disabled }: HoverLayerOptions) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const wrapper = useRef<HTMLSpanElement>(null);
  const timer = useRef<number | undefined>(undefined);

  const clear = () => {
    window.clearTimeout(timer.current);
  };
  const show = (delay: number) => {
    clear();
    const target = wrapper.current?.firstElementChild;
    if (disabled || !(target instanceof HTMLElement)) {
      return;
    }
    const warm = !interactive && Date.now() - lastClosedAt < WARM_WINDOW_MS;
    timer.current = window.setTimeout(
      () => {
        setAnchor(target);
      },
      warm ? 0 : delay,
    );
  };
  const hide = (delay: number) => {
    clear();
    timer.current = window.setTimeout(() => {
      setAnchor((current) => {
        if (current && !interactive) {
          lastClosedAt = Date.now();
        }
        return null;
      });
    }, delay);
  };
  useEffect(() => clear, []);

  const outside = (related: EventTarget | null) => !(related instanceof Node && wrapper.current?.contains(related));

  return {
    anchor,
    wrapper,
    triggerProps: {
      onPointerOver: () => {
        show(openDelay);
      },
      onPointerOut: (event: PointerEvent) => {
        if (outside(event.relatedTarget)) {
          hide(closeDelay);
        }
      },
      onFocus: (event: FocusEvent) => {
        if (focusIsVisible(event.target)) {
          show(openDelay);
        }
      },
      onBlur: () => {
        hide(0);
      },
      onPointerDown: () => {
        hide(0);
      },
      onKeyDown: (event: KeyboardEvent) => {
        if (event.key === "Escape" && anchor) {
          event.stopPropagation();
          hide(0);
        }
      },
    },
    layerProps: interactive
      ? {
          onPointerEnter: clear,
          onPointerLeave: () => {
            hide(closeDelay);
          },
        }
      : {},
    close: useCallback(() => {
      setAnchor(null);
    }, []),
  };
}

/**
 * Points the trigger at the description while mounted (the wrapper adds no element of its own).
 * The description lives in a hidden node at the end of <body>, not inside the trigger.
 */
function useDescribedBy(wrapper: RefObject<HTMLSpanElement | null>, id: string, enabled: boolean): void {
  useEffect(() => {
    const target = wrapper.current?.firstElementChild;
    if (!enabled || !target) {
      return undefined;
    }
    const previous = target.getAttribute("aria-describedby");
    target.setAttribute("aria-describedby", previous ? `${previous} ${id}` : id);
    return () => {
      if (previous === null) {
        target.removeAttribute("aria-describedby");
      } else {
        target.setAttribute("aria-describedby", previous);
      }
    };
  }, [wrapper, id, enabled]);
}

export interface TooltipProps {
  /** Short text. With `describe`, it is also the trigger's accessible description. */
  readonly content: ReactNode;
  readonly side?: Side;
  readonly align?: Align;
  /**
   * Whether screen readers hear the content as a description. Turn off when it repeats the
   * trigger's own label (icon buttons).
   */
  readonly describe?: boolean;
  readonly disabled?: boolean;
  /** One element; it receives hover and focus. */
  readonly children: ReactNode;
}

/** Names a control after a short delay. Nearby tooltips open at once while one was just showing. */
export function Tooltip({ content, side = "bottom", align, describe = true, disabled = false, children }: TooltipProps) {
  const id = useId();
  const { anchor, wrapper, triggerProps } = useHoverLayer({
    openDelay: 500,
    closeDelay: 0,
    interactive: false,
    disabled,
  });
  useDescribedBy(wrapper, `${id}-description`, describe);
  const layer = useFloatingPosition<HTMLDivElement>(anchor, { side, ...(align ? { align } : {}) }, anchor !== null);

  return (
    <>
      <span className="hover-trigger" ref={wrapper} {...triggerProps}>
        {children}
      </span>
      {describe ? (
        // Outside the trigger's subtree (in <body>), so it never becomes part of the trigger's text:
        // copying a code row that holds a tooltip copies exactly the code.
        <Portal>
          <span id={`${id}-description`} hidden>
            {content}
          </span>
        </Portal>
      ) : null}
      {anchor ? (
        <Portal>
          <div ref={layer} className="tooltip" role="tooltip" aria-hidden="true">
            <span>{content}</span>
          </div>
        </Portal>
      ) : null}
    </>
  );
}

export interface HoverCardProps {
  /** Rendered only while open, so it may fetch what it shows. */
  readonly content: ReactNode;
  readonly side?: Side;
  readonly align?: Align;
  readonly openDelay?: number;
  readonly disabled?: boolean;
  readonly className?: string;
  readonly children: ReactNode;
}

/** Richer hover detail; the pointer can move into it, and it stays while hovered. */
export function HoverCard({
  content,
  side = "bottom",
  align = "start",
  openDelay = 400,
  disabled = false,
  className,
  children,
}: HoverCardProps) {
  const id = useId();
  const { anchor, wrapper, triggerProps, layerProps, close } = useHoverLayer({
    openDelay,
    closeDelay: 150,
    interactive: true,
    disabled,
  });
  const layer = useFloatingPosition<HTMLDivElement>(anchor, { side, align }, anchor !== null);
  useEffect(() => {
    const target = wrapper.current?.firstElementChild;
    if (!anchor || !target) {
      return undefined;
    }
    target.setAttribute("aria-describedby", id);
    return () => {
      target.removeAttribute("aria-describedby");
    };
  }, [anchor, wrapper, id]);

  return (
    <>
      <span className="hover-trigger" ref={wrapper} {...triggerProps}>
        {children}
      </span>
      {anchor ? (
        <Portal>
          <div
            ref={layer}
            id={id}
            className={cx("hovercard", className)}
            role="tooltip"
            {...layerProps}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                close();
              }
            }}
          >
            {content}
          </div>
        </Portal>
      ) : null}
    </>
  );
}
