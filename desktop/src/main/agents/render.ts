/**
 * Page images and page checks. A hidden render window draws one page of a
 * board read-only, the way the board view draws it, from the page the main process hands it; the main
 * process then captures it (`screenshot`, and the board menu's Export PNG…), or takes the layout and content
 * checks the window ran on the drawn page and adds the board's health (`check`, `checkBoard`).
 *
 * - **The page** comes from the window that holds the board (unsaved edits included) or, for a board
 *   the window doesn't hold, or when it is closed, from the engine (`read_board`, which loads no
 *   session). The board's health is the engine's `board_health`: the session's while the board is
 *   open, else its file's. Reading never changes anything: an image or a check opens neither the
 *   window nor a board session, and the person's view doesn't move.
 * - **The render window** is created on demand with every window's hardening, offscreen (it never
 *   shows) at the image's scale, and attached to the engine for the cards' data. It holds no board
 *   session, and may send only `render_ready`. Requests run one at a time; it closes after a minute without
 *   one, and at once when it stops answering.
 * - **Show all calls** is as the person's window shows it, read at each request, so an image or a check
 *   draws builtin calls when the person sees them.
 * - **An image** is drawn at zoom 1 when it fits, else smaller, to keep the long side at 4,096 pixels
 *   or less (the note says so): the window measures the page, the main process sizes the window and
 *   chooses the camera, the window draws it, and the main process captures it.
 */

import { BrowserWindow, type WebContents } from "electron";
import type { AgentBoardRef, AgentFailure, AgentPage } from "../../shared/agent-api";
import type { PageCheck, PageCheckKind, RenderAnswer, RenderCamera, RenderRect, RenderShellEvent } from "../../shared/render-api";
import { IPC } from "../../shared/transport";
import { renderUrl, type UiSource } from "../app-url";
import { EngineRequestError, type EngineHost } from "../engine-host";
import type { BoardHealthView, HealthItem, ReadBoardResult } from "../mcp/engine-types";
import { hardenedPreferences, type WindowEntry } from "../windows";
import type { AgentBridge } from "./bridge";

/** The longest side an image may have, in pixels. */
const MAX_IMAGE_SIDE = 4096;
/** Room around what an image shows, in CSS pixels. */
const IMAGE_PADDING = 24;
/** The render window's size while it measures a page, in CSS pixels. */
const MEASURE_SIZE = { width: 1600, height: 1000 } as const;
/** How long a page may take to load and be measured (a resting project wakes first). */
const RENDER_TIMEOUT_MS = 30_000;
/** How long a measured page may take to be drawn with its camera. */
const FRAME_TIMEOUT_MS = 15_000;
/** How long the render window stays open without a request. */
const IDLE_MS = 60_000;

const CHECK_KINDS: ReadonlySet<string> = new Set<PageCheckKind>(["overlap", "crossings", "density", "missingWords", "evidence", "strays", "wordy"]);

/** An image's scale: CSS pixels to image pixels. */
export type ImageScale = 1 | 2;

/** What to take an image of: a page, or one card, frame, resource card, or note on it. */
export interface ScreenshotRequest {
  readonly board: AgentBoardRef;
  readonly page: string;
  readonly item?: string;
  /** 1 (the default) or 2, for a sharper image. */
  readonly scale?: ImageScale;
}

/** A PNG of a page or an item, its size in pixels, and a note when it isn't drawn at full size or misses something. */
export type ScreenshotOutcome =
  | { readonly ok: true; readonly png: Buffer; readonly width: number; readonly height: number; readonly note?: string }
  | AgentFailure;

/** One page's health items (each with the repairs it offers) and page checks. */
export interface PageChecks {
  readonly page: string;
  readonly pageTitle: string;
  readonly health: readonly HealthItem[];
  readonly checks: readonly PageCheck[];
}

/** A page's checks, with its board's title. */
export type PageChecksOutcome = ({ readonly ok: true; readonly title: string } & PageChecks) | AgentFailure;

/** Every page's checks, in page order, with the board's title. */
export type BoardChecksOutcome = { readonly ok: true; readonly title: string; readonly pages: readonly PageChecks[] } | AgentFailure;

/** How an image of some bounds is taken: the window's size, the camera, and the zoom. */
export interface ImageFrame {
  /** The window's content size, in CSS pixels. */
  readonly width: number;
  readonly height: number;
  readonly camera: RenderCamera;
  readonly zoom: number;
}

/**
 * How an image of `bounds` (canvas units) is taken at `scale`, with padding around it: at zoom 1 when
 * it fits `room` (CSS pixels; the image's longest side over the scale), else at the zoom that fits.
 */
export function imageFrame(bounds: RenderRect, scale: ImageScale, room: { readonly width: number; readonly height: number } = { width: MAX_IMAGE_SIDE / scale, height: MAX_IMAGE_SIDE / scale }): ImageFrame {
  const zoom = Math.min(1, (room.width - 2 * IMAGE_PADDING) / Math.max(1, bounds.width), (room.height - 2 * IMAGE_PADDING) / Math.max(1, bounds.height));
  // Whole pixels, rounded up, but never past the room by a rounding error.
  const pixels = (length: number) => Math.ceil(length * zoom + 2 * IMAGE_PADDING - 1e-6);
  return {
    width: pixels(bounds.width),
    height: pixels(bounds.height),
    camera: { x: IMAGE_PADDING - bounds.x * zoom, y: IMAGE_PADDING - bounds.y * zoom, zoom },
    zoom,
  };
}

/** What the render window needs of the engine host. */
type RenderEngine = Pick<EngineHost, "attach" | "detach" | "untrusted">;

export interface PageRendererOptions {
  readonly ui: UiSource;
  readonly bridge: Pick<AgentBridge, "read">;
  readonly engine: RenderEngine;
  /** A window id from the app window's sequence (`AppWindow.reserveId`). */
  readonly reserveId: () => number;
  /** The Show all calls switch of the person's window (`Workspace.showAllCalls`). */
  readonly showAllCalls: () => boolean;
  /** How long a page may take to load and be measured. */
  readonly timeoutMs?: number;
  /** How long the render window stays open without a request. */
  readonly idleMs?: number;
}

/** The page a request draws, and its board's title. */
interface PageSource {
  readonly ok: true;
  readonly page: AgentPage;
  readonly title: string;
}

/** A board's health, or why it couldn't be read. */
type HealthOutcome = { readonly ok: true; readonly view: BoardHealthView } | AgentFailure;

interface RenderWindow {
  readonly entry: WindowEntry;
  readonly scale: ImageScale;
  /** Shell events waiting for the window's UI to load. */
  readonly queued: RenderShellEvent[];
}

interface Pending {
  /** The window asked. */
  readonly window: RenderWindow;
  readonly requestId: number;
  readonly resolve: (answer: RenderAnswer) => void;
  readonly timer: ReturnType<typeof setTimeout>;
}

function failure(code: string, message: string): AgentFailure {
  return { ok: false, code, message };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isRect(value: unknown): value is RenderRect {
  return isRecord(value) && ["x", "y", "width", "height"].every((key) => Number.isFinite(value[key])) && Number(value["width"]) > 0 && Number(value["height"]) > 0;
}

function isCheck(value: unknown): value is PageCheck {
  return (
    isRecord(value) &&
    typeof value["kind"] === "string" &&
    CHECK_KINDS.has(value["kind"]) &&
    Array.isArray(value["items"]) &&
    value["items"].every((item) => typeof item === "string") &&
    typeof value["message"] === "string" &&
    typeof value["suggestion"] === "string"
  );
}

/** The render window's answer, checked enough that callers can rely on its shape. */
function checked(result: unknown): RenderAnswer {
  if (isRecord(result) && result["ok"] === false && typeof result["code"] === "string" && typeof result["message"] === "string") {
    return failure(result["code"], result["message"]);
  }
  if (isRecord(result) && result["ok"] === true) {
    const { bounds, checks } = result;
    if ((bounds === undefined || isRect(bounds)) && (checks === undefined || (Array.isArray(checks) && checks.every(isCheck)))) {
      return { ok: true, ...(bounds !== undefined ? { bounds } : {}), ...(checks !== undefined ? { checks } : {}) };
    }
  }
  return failure("internal", "The render window answered with a malformed result.");
}

/** Page `page` of a board's file, or a refusal that lists the pages it has. */
function filePage(file: ReadBoardResult, page: string): PageSource | AgentFailure {
  const found = file.document.pages.find((item) => item.id === page);
  if (!found) {
    const pages = file.document.pages.map((item) => `\`${item.id}\` (${item.title})`).join(", ");
    return failure("page_not_found", `The board has no page \`${page}\`. Its pages: ${pages}.`);
  }
  return { ok: true, page: found, title: file.document.title };
}

/** An engine failure as a refusal (a missing board file reads `board_not_found`); anything else is thrown again. */
function engineFailure(error: unknown, board: AgentBoardRef): AgentFailure {
  if (!(error instanceof EngineRequestError)) {
    throw error;
  }
  const { code, message } = error.error;
  return code === "not_found" ? failure("board_not_found", `The project has no board \`${board.id}\`.`) : failure(code, message);
}

/** `Omit` over each member of a union. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

export class PageRenderer {
  private readonly options: PageRendererOptions;
  private current: RenderWindow | null = null;
  private pending: Pending | null = null;
  /** The requests so far, run one after another. */
  private queue: Promise<unknown> = Promise.resolve();
  private waiting = 0;
  private idle: ReturnType<typeof setTimeout> | null = null;
  private nextId = 1;

  constructor(options: PageRendererOptions) {
    this.options = options;
  }

  /** A PNG of a page, or of one card, frame, resource card, or note on it. */
  screenshot(request: ScreenshotRequest): Promise<ScreenshotOutcome> {
    return this.serialized(() => this.takeImage(request));
  }

  /** A page's health items and page checks. */
  check(request: { readonly board: AgentBoardRef; readonly page: string }): Promise<PageChecksOutcome> {
    return this.serialized(async () => {
      const source = await this.source(request.board, request.page);
      if (!source.ok) {
        return source;
      }
      const health = await this.health(request.board);
      if (!health.ok) {
        return health;
      }
      const checked = await this.checkPage(request.board, source, health.view);
      return checked.ok ? { ...checked, title: source.title } : checked;
    });
  }

  /** Every page's health items and page checks, in page order. */
  checkBoard(request: { readonly board: AgentBoardRef }): Promise<BoardChecksOutcome> {
    return this.serialized(async () => {
      const { board } = request;
      const file = await this.file(board);
      if (!file.ok) {
        return file;
      }
      const health = await this.health(board);
      if (!health.ok) {
        return health;
      }
      const pages: PageChecks[] = [];
      let title = file.result.document.title;
      for (const { id: page } of file.result.document.pages) {
        const source = (await this.held(board, page)) ?? filePage(file.result, page);
        if (!source.ok) {
          return source;
        }
        title = source.title;
        const checked = await this.checkPage(board, source, health.view);
        if (!checked.ok) {
          return checked;
        }
        const { ok: _, ...result } = checked;
        pages.push(result);
      }
      return { ok: true, title, pages };
    });
  }

  /** The render window answered request `requestId`; any other answer is ignored. */
  settle(requestId: unknown, result: unknown): void {
    const pending = this.pending;
    if (!pending || pending.requestId !== requestId) {
      return;
    }
    clearTimeout(pending.timer);
    this.pending = null;
    pending.resolve(checked(result));
  }

  /** Whether `contents` is the render window's. */
  owns(contents: WebContents): boolean {
    const window = this.current?.entry.window;
    return window !== undefined && !window.isDestroyed() && window.webContents === contents;
  }

  /** The render window, once loaded, for the engine host to attach again after a restart. */
  entries(): WindowEntry[] {
    return this.current?.entry.loaded === true ? [this.current.entry] : [];
  }

  /** Closes the render window (it opens again for the next request). */
  close(): void {
    this.stopIdle();
    const window = this.current?.entry.window;
    this.current = null;
    if (window && !window.isDestroyed()) {
      window.close();
    }
  }

  private serialized<T>(work: () => Promise<T>): Promise<T> {
    this.waiting += 1;
    this.stopIdle();
    const run = this.queue.then(work).finally(() => {
      this.waiting -= 1;
      if (this.waiting === 0 && this.current) {
        this.idle = setTimeout(() => {
          this.close();
        }, this.options.idleMs ?? IDLE_MS);
      }
    });
    this.queue = run.catch(() => undefined);
    return run;
  }

  private stopIdle(): void {
    if (this.idle !== null) {
      clearTimeout(this.idle);
      this.idle = null;
    }
  }

  private async takeImage({ board, page, item, scale = 1 }: ScreenshotRequest): Promise<ScreenshotOutcome> {
    const source = await this.source(board, page);
    if (!source.ok) {
      return source;
    }
    const health = await this.health(board);
    const window = this.window(scale);
    const drawn = await this.ask(
      window,
      {
        type: "render_page",
        purpose: "image",
        board,
        title: source.title,
        page: source.page,
        health: health.ok ? health.view.health : null,
        showAllCalls: this.options.showAllCalls(),
        ...(item !== undefined ? { item } : {}),
      },
      this.options.timeoutMs ?? RENDER_TIMEOUT_MS,
    );
    if (!drawn.ok) {
      return drawn;
    }
    if (!drawn.bounds) {
      return failure("internal", "The render window answered without the bounds to capture.");
    }
    const browser = window.entry.window;
    let frame = imageFrame(drawn.bounds, scale);
    browser.setContentSize(frame.width, frame.height);
    // A window larger than the screen may be made smaller: then the image fits what it got.
    const [width = frame.width, height = frame.height] = browser.getContentSize();
    if (width < frame.width || height < frame.height) {
      frame = imageFrame(drawn.bounds, scale, { width, height });
    }
    const framed = await this.ask(window, { type: "render_frame", camera: frame.camera, width: frame.width, height: frame.height }, FRAME_TIMEOUT_MS);
    if (!framed.ok) {
      return framed;
    }
    const image = await browser.webContents.capturePage({ x: 0, y: 0, width: frame.width, height: frame.height }, { stayHidden: true });
    const size = image.getSize(scale);
    const notes = [
      ...(frame.zoom < 1 ? [`Drawn at ${Math.round(frame.zoom * 100)}% to fit ${MAX_IMAGE_SIDE.toLocaleString("en-US")} px on its long side; take a frame's or a card's image to read it closer.`] : []),
      ...(health.ok ? [] : [`No review banners: the board's health couldn't be read (${health.message}).`]),
    ];
    return { ok: true, png: image.toPNG({ scaleFactor: scale }), width: size.width, height: size.height, ...(notes.length > 0 ? { note: notes.join(" ") } : {}) };
  }

  /** Runs the page checks on a page and picks its health items from its board's. */
  private async checkPage(board: AgentBoardRef, source: PageSource, health: BoardHealthView): Promise<({ readonly ok: true } & PageChecks) | AgentFailure> {
    const answer = await this.ask(
      this.window(),
      { type: "render_page", purpose: "check", board, title: source.title, page: source.page, health: health.health, showAllCalls: this.options.showAllCalls() },
      this.options.timeoutMs ?? RENDER_TIMEOUT_MS,
    );
    if (!answer.ok) {
      return answer;
    }
    const items = health.health.items.filter((item) => item.page === source.page.id);
    return { ok: true, page: source.page.id, pageTitle: source.page.title, health: items, checks: answer.checks ?? [] };
  }

  /** The page to draw: as the window that holds the board has it, else from the board's file. */
  private async source(board: AgentBoardRef, page: string): Promise<PageSource | AgentFailure> {
    const held = await this.held(board, page);
    if (held !== null) {
      return held;
    }
    const file = await this.file(board);
    return file.ok ? filePage(file.result, page) : file;
  }

  /**
   * The page as the window that holds the board has it (unsaved edits included); null when no window
   * holds it, or the window closes as it reads, never opening it again.
   */
  private async held(board: AgentBoardRef, page: string): Promise<PageSource | AgentFailure | null> {
    const read = await this.options.bridge.read({ board, page });
    if (read.ok) {
      return { ok: true, page: read.page, title: read.board.title };
    }
    return read.code === "board_not_open" || read.code === "window_closed" ? null : read;
  }

  /** The board as its file holds it, without loading its session. */
  private async file(board: AgentBoardRef): Promise<{ readonly ok: true; readonly result: ReadBoardResult } | AgentFailure> {
    try {
      return { ok: true, result: (await this.options.engine.untrusted(0, "read_board", { project: board.project, id: board.id })) as ReadBoardResult };
    } catch (error) {
      return engineFailure(error, board);
    }
  }

  /** The board's health: its session's while a window holds it, else its file's (`board_health` opens nothing). */
  private async health(board: AgentBoardRef): Promise<HealthOutcome> {
    try {
      return { ok: true, view: (await this.options.engine.untrusted(0, "board_health", { project: board.project, id: board.id })) as BoardHealthView };
    } catch (error) {
      return engineFailure(error, board);
    }
  }

  /** The render window at `scale` (any, for checks): the open one, or a new one. */
  private window(scale?: ImageScale): RenderWindow {
    const current = this.current;
    if (current && !current.entry.window.isDestroyed() && (scale === undefined || current.scale === scale)) {
      current.entry.window.setContentSize(MEASURE_SIZE.width, MEASURE_SIZE.height);
      return current;
    }
    this.close();
    const created = this.create(scale ?? 1);
    this.current = created;
    return created;
  }

  private create(scale: ImageScale): RenderWindow {
    const window = new BrowserWindow({
      show: false,
      width: MEASURE_SIZE.width,
      height: MEASURE_SIZE.height,
      useContentSize: true,
      enableLargerThanScreen: true,
      frame: false,
      skipTaskbar: true,
      webPreferences: { ...hardenedPreferences(false), offscreen: { deviceScaleFactor: scale }, backgroundThrottling: false },
    });
    const entry: WindowEntry = { id: this.options.reserveId(), window, loaded: false };
    const created: RenderWindow = { entry, scale, queued: [] };
    window.webContents.on("did-start-loading", () => {
      entry.loaded = false;
    });
    window.webContents.on("did-finish-load", () => {
      entry.loaded = true;
      this.options.engine.attach(entry);
      for (const event of created.queued.splice(0)) {
        window.webContents.send(IPC.shellEvent, event);
      }
    });
    window.webContents.on("render-process-gone", () => {
      window.close();
    });
    window.on("closed", () => {
      this.options.engine.detach(entry);
      if (this.current === created) {
        this.current = null;
      }
      const pending = this.pending;
      if (pending?.window === created) {
        clearTimeout(pending.timer);
        this.pending = null;
        pending.resolve(failure("render_failed", "The render window closed before it finished the page. Try again."));
      }
    });
    void window.loadURL(renderUrl(this.options.ui));
    return created;
  }

  /** Sends the render window a request (held until its UI has loaded) and waits for its answer. */
  private ask(window: RenderWindow, event: DistributiveOmit<RenderShellEvent, "requestId">, timeoutMs: number): Promise<RenderAnswer> {
    const requestId = this.nextId++;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pending = null;
        // A window that stopped answering may be stuck: the next request gets a new one.
        this.close();
        resolve(failure("render_timeout", `Unfold couldn't draw the page within ${Math.round(timeoutMs / 1000)} s. Try again; a page with many cards takes longer.`));
      }, timeoutMs);
      this.pending = { window, requestId, resolve, timer };
      const message = { ...event, requestId } as RenderShellEvent;
      if (window.entry.loaded) {
        window.entry.window.webContents.send(IPC.shellEvent, message);
      } else {
        window.queued.push(message);
      }
    });
  }
}
