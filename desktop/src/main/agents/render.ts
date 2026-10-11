/**
 * Page images and page checks (design §5.1, §6.5, §7.7). A hidden render window draws one page of a
 * board read-only, the way a board tab draws it, from the page the main process hands it; the main
 * process then captures it (`screenshot`, and Share › Export PNG), or takes the layout and content
 * checks the window ran on the drawn page and adds the board's health (`check`, `checkBoard`).
 *
 * - **The page** comes from the window that holds the board (unsaved edits included) or, for an image
 *   of a board no window holds, from the engine (`read_board`, which claims nothing). Health needs the
 *   board's session (`board_health` runs for the holding window), so a check opens a board no window
 *   holds in a background tab first, as an edit does; an image of such a board has no review banners.
 * - **The render window** is created on demand with every window's hardening, offscreen (it never
 *   shows) at the image's scale, and attached to the engine for the cards' data. It claims nothing,
 *   and may send only `render_ready`. Requests run one at a time; it closes after a minute without
 *   one, and at once when it stops answering.
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

const CHECK_KINDS: ReadonlySet<string> = new Set<PageCheckKind>(["overlap", "crossings", "density", "unlabeled", "missingWords", "evidence", "strays", "views"]);

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
  readonly bridge: Pick<AgentBridge, "read" | "open" | "views">;
  readonly engine: RenderEngine;
  /** A window id from the windows' sequence (`WindowRegistry.reserveId`). */
  readonly reserveId: () => number;
  /** How long a page may take to load and be measured. */
  readonly timeoutMs?: number;
  /** How long the render window stays open without a request. */
  readonly idleMs?: number;
}

/** The page a request draws, with what it needs besides. */
interface PageSource {
  readonly ok: true;
  readonly page: AgentPage;
  /** The board's title. */
  readonly title: string;
  /** The board's health, when a window holds it. */
  readonly health: BoardHealthView | null;
}

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

  /** A page's health items and page checks (opening the board in a background tab when no window holds it). */
  check(request: { readonly board: AgentBoardRef; readonly page: string }): Promise<PageChecksOutcome> {
    return this.serialized(async () => {
      const source = await this.source(request.board, request.page, true);
      if (!source.ok) {
        return source;
      }
      const checked = await this.checkPage(request.board, source);
      return checked.ok ? { ...checked, title: source.title } : checked;
    });
  }

  /** Every page's health items and page checks, in page order (opening the board when no window holds it). */
  checkBoard(request: { readonly board: AgentBoardRef }): Promise<BoardChecksOutcome> {
    return this.serialized(async () => {
      const { board } = request;
      const opened = await this.options.bridge.open({ board });
      if (!opened.ok) {
        return opened;
      }
      const health = await this.health(opened.windowId, board);
      if (!health.ok) {
        return health;
      }
      const pages: PageChecks[] = [];
      let title = "";
      for (const { page } of health.view.outlines) {
        const read = await this.options.bridge.read({ board, page });
        if (!read.ok) {
          return read;
        }
        title = read.board.title;
        const checked = await this.checkPage(board, { ok: true, page: read.page, title, health: health.view });
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
    const source = await this.source(board, page, false);
    if (!source.ok) {
      return source;
    }
    const window = this.window(scale);
    const drawn = await this.ask(
      window,
      { type: "render_page", purpose: "image", board, title: source.title, page: source.page, health: source.health?.health ?? null, ...(item !== undefined ? { item } : {}) },
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
      ...(source.health === null ? ["No review banners: no window holds the board, so its health isn't known here (check opens it and reports it)."] : []),
    ];
    return { ok: true, png: image.toPNG({ scaleFactor: scale }), width: size.width, height: size.height, ...(notes.length > 0 ? { note: notes.join(" ") } : {}) };
  }

  /** Runs the page checks on a page and picks its health items. */
  private async checkPage(board: AgentBoardRef, source: PageSource): Promise<({ readonly ok: true } & PageChecks) | AgentFailure> {
    const answer = await this.ask(
      this.window(),
      { type: "render_page", purpose: "check", board, title: source.title, page: source.page, health: source.health?.health ?? null },
      this.options.timeoutMs ?? RENDER_TIMEOUT_MS,
    );
    if (!answer.ok) {
      return answer;
    }
    const health = (source.health?.health.items ?? []).filter((item) => item.page === source.page.id);
    return { ok: true, page: source.page.id, pageTitle: source.page.title, health, checks: answer.checks ?? [] };
  }

  /**
   * The page to draw: from the window that holds the board (`open`: opening it in a background tab
   * when none does), with its health; or, when no window holds it, from the board's file.
   */
  private async source(board: AgentBoardRef, page: string, open: boolean): Promise<PageSource | AgentFailure> {
    let holder = this.options.bridge.views.holder(board);
    if (holder === undefined && open) {
      const opened = await this.options.bridge.open({ board, page });
      if (!opened.ok) {
        return opened;
      }
      holder = opened.windowId;
    }
    if (holder === undefined) {
      return this.fromFile(board, page);
    }
    const read = await this.options.bridge.read({ board, page });
    if (!read.ok) {
      return read.code === "board_not_open" && !open ? this.fromFile(board, page) : read;
    }
    const health = await this.health(holder, board);
    if (!health.ok && open) {
      return health;
    }
    return { ok: true, page: read.page, title: read.board.title, health: health.ok ? health.view : null };
  }

  /** The page as the board's working file has it, without claiming the board. */
  private async fromFile(board: AgentBoardRef, page: string): Promise<PageSource | AgentFailure> {
    let result: ReadBoardResult;
    try {
      result = (await this.options.engine.untrusted(0, "read_board", { project: board.project, id: board.id })) as ReadBoardResult;
    } catch (error) {
      return engineFailure(error, board);
    }
    const found = result.document.pages.find((item) => item.id === page);
    if (!found) {
      const pages = result.document.pages.map((item) => `\`${item.id}\` (${item.title})`).join(", ");
      return failure("page_not_found", `The board has no page \`${page}\`. Its pages: ${pages}.`);
    }
    return { ok: true, page: found, title: result.document.title, health: null };
  }

  /** The board's health and pages' outlines, as the window holding it sees them. */
  private async health(windowId: number, board: AgentBoardRef): Promise<{ readonly ok: true; readonly view: BoardHealthView } | AgentFailure> {
    try {
      const view = (await this.options.engine.untrusted(windowId, "board_health", { project: board.project, id: board.id })) as BoardHealthView;
      return { ok: true, view };
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
    const entry: WindowEntry = { id: this.options.reserveId(), window, key: "render", loaded: false };
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
