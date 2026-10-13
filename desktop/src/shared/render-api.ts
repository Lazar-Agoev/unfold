/**
 * Page images and page checks (design §5.1, §6.5, §7.7): a hidden render window draws one page of a
 * board read-only, the way the board view draws it, from what the main process hands it. The main
 * process then captures it (an image), or takes the layout and content checks the window ran on the
 * laid-out page. Types only, so the UI imports them as `@unfold/desktop/render-api`.
 *
 * - main → render window: the shell events of {@link RenderShellEvent};
 * - render window → main: the window command `render_ready` ({@link RenderWindowCommands}), the only
 *   command the render window may send.
 */

import type { AgentBoardRef, AgentPage } from "./agent-api";

/** A rectangle on the canvas, in canvas units. */
export interface RenderRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** The canvas camera: React Flow's viewport. */
export interface RenderCamera {
  readonly x: number;
  readonly y: number;
  readonly zoom: number;
}

/** A board's health as the engine reports it (the UI's generated `Health`); plain JSON on this side. */
export interface RenderHealth {
  readonly items: readonly unknown[];
  readonly [field: string]: unknown;
}

/** One page to draw, and what for. */
export interface RenderJob {
  readonly requestId: number;
  /** `image`: report the bounds of what to capture (then a {@link RenderFrame} follows); `check`: run the page checks. */
  readonly purpose: "image" | "check";
  readonly board: AgentBoardRef;
  /** The board's title. */
  readonly title: string;
  /** The page as the window holding the board has it (unsaved edits included), or as its file has it. */
  readonly page: AgentPage;
  /** The board's health, which the cards' review banners show; null when it isn't known. */
  readonly health: RenderHealth | null;
  /** An image of one item (a card, or a frame, resource card, or note as drawn) instead of the page. */
  readonly item?: string;
}

/** Takes the camera an image is captured with, once the window is the size the main process chose. */
export interface RenderFrame {
  readonly requestId: number;
  readonly camera: RenderCamera;
  /** The window's content size, in CSS pixels. */
  readonly width: number;
  readonly height: number;
}

/** A layout or content problem the page checks found (design §7.7). */
export type PageCheckKind = "overlap" | "crossings" | "density" | "unlabeled" | "missingWords" | "evidence" | "strays" | "views";

/** One finding of the page checks: what it is about, in words, and what fixes it. */
export interface PageCheck {
  readonly kind: PageCheckKind;
  /** The ids of the page items it is about, in page order. */
  readonly items: readonly string[];
  /** What is wrong, with the items' names. */
  readonly message: string;
  /** What fixes it, naming the tool to call. */
  readonly suggestion: string;
}

/**
 * The render window's answer: to a job, the bounds to capture (an image) or the checks; to a frame,
 * that it is drawn. A refusal says why (`empty_page`, `item_not_found`, `item_hidden`, `render_timeout`).
 */
export type RenderAnswer =
  | { readonly ok: true; readonly bounds?: RenderRect; readonly checks?: readonly PageCheck[] }
  | { readonly ok: false; readonly code: string; readonly message: string };

/** A request as a shell event: its `type` and its fields (flattened, so it reads as any event does). */
type ShellEventOf<T extends string, P> = { readonly type: T } & { readonly [K in keyof P]: P[K] };

/** Shell events the main process sends the render window. */
export type RenderShellEvent = ShellEventOf<"render_page", RenderJob> | ShellEventOf<"render_frame", RenderFrame>;

/** The render window's one window command (part of `WindowCommands`). */
export interface RenderWindowCommands {
  /** The answer to a `render_page` or `render_frame`. */
  render_ready: { args: { requestId: number; result: RenderAnswer }; result: null };
}
