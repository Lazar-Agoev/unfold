/**
 * The app's own origin, `app://unfold/`, and the content security policies. Pure functions, so the
 * path confinement and the origin checks are tested without Electron.
 */

import { extname, resolve, sep } from "node:path";

export const APP_SCHEME = "app";
const APP_HOST = "unfold";
const APP_ENTRY_URL = `${APP_SCHEME}://${APP_HOST}/index.html`;
const APP_RENDER_URL = `${APP_SCHEME}://${APP_HOST}/render.html`;

/**
 * The production policy: every asset comes from the app itself, there are no inline scripts or
 * styles, and nothing is loaded from the network (docs/architecture.md#security).
 */
export const APP_CONTENT_SECURITY_POLICY = [
  "default-src 'none'",
  "script-src 'self'",
  "style-src 'self'",
  "img-src 'self'",
  "font-src 'self'",
  "connect-src 'self'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
  "object-src 'none'",
].join("; ");

/**
 * The development policy for the Vite dev server: the same, except that the dev server's origin and
 * its hot-reload socket are allowed, and React Refresh needs its inline preamble script.
 */
export function devContentSecurityPolicy(devOrigin: string): string {
  const socket = devOrigin.replace(/^http/, "ws");
  return [
    "default-src 'none'",
    `script-src 'self' 'unsafe-inline' ${devOrigin}`,
    `style-src 'self' 'unsafe-inline' ${devOrigin}`,
    `img-src 'self' ${devOrigin}`,
    `font-src 'self' ${devOrigin}`,
    `connect-src 'self' ${devOrigin} ${socket}`,
    "base-uri 'none'",
    "form-action 'none'",
    "frame-ancestors 'none'",
    "object-src 'none'",
  ].join("; ");
}

/** Where windows load the UI from. */
export type UiSource = { readonly kind: "app"; readonly dir: string } | { readonly kind: "dev"; readonly url: string };

/** The URL a new window loads. */
export function entryUrl(source: UiSource): string {
  return source.kind === "app" ? APP_ENTRY_URL : source.url;
}

/** The URL the hidden render window loads (page images and page checks). */
export function renderUrl(source: UiSource): string {
  return source.kind === "app" ? APP_RENDER_URL : new URL("render.html", source.url).toString();
}

/** True when `url` belongs to the UI the main process loaded (the only frames it trusts). */
export function isUiUrl(source: UiSource, url: string): boolean {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  if (source.kind === "app") {
    return parsed.protocol === `${APP_SCHEME}:` && parsed.host === APP_HOST;
  }
  return parsed.origin === new URL(source.url).origin;
}

/**
 * Maps an `app://unfold/...` request to a file inside `uiDir`, or null if the request names another
 * host or would escape the directory. `/` serves `index.html`.
 */
export function resolveAppFile(uiDir: string, requestUrl: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(requestUrl);
  } catch {
    return null;
  }
  if (parsed.protocol !== `${APP_SCHEME}:` || parsed.host !== APP_HOST) {
    return null;
  }
  let pathname: string;
  try {
    pathname = decodeURIComponent(parsed.pathname);
  } catch {
    return null;
  }
  if (pathname.includes("\0")) {
    return null;
  }
  const root = resolve(uiDir);
  const file = resolve(root, `.${pathname === "/" ? "/index.html" : pathname}`);
  return file.startsWith(root + sep) ? file : null;
}

const CONTENT_TYPES: Readonly<Record<string, string>> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".map": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".ttf": "font/ttf",
};

/** The content type for a bundled file, by extension. */
export function contentType(file: string): string {
  return CONTENT_TYPES[extname(file).toLowerCase()] ?? "application/octet-stream";
}
