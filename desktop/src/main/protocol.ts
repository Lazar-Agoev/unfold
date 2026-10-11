/**
 * The privileged `app://` scheme that serves the built UI from inside the app, with a strict content
 * security policy on every response (docs/architecture.md#security).
 */

import { readFile } from "node:fs/promises";
import { protocol, session } from "electron";
import {
  APP_CONTENT_SECURITY_POLICY,
  APP_SCHEME,
  contentType,
  devContentSecurityPolicy,
  resolveAppFile,
  type UiSource,
} from "./app-url";

/** Must run before the app is ready. */
export function registerAppScheme(): void {
  protocol.registerSchemesAsPrivileged([
    {
      scheme: APP_SCHEME,
      privileges: { standard: true, secure: true, supportFetchAPI: true, codeCache: true },
    },
  ]);
}

/** Serves the UI: from `app://` in production, or adds the dev policy to the Vite dev server. */
export function serveUi(source: UiSource): void {
  if (source.kind === "dev") {
    const origin = new URL(source.url).origin;
    const policy = devContentSecurityPolicy(origin);
    session.defaultSession.webRequest.onHeadersReceived({ urls: [`${origin}/*`] }, (details, callback) => {
      callback({ responseHeaders: { ...details.responseHeaders, "Content-Security-Policy": [policy] } });
    });
    return;
  }

  protocol.handle(APP_SCHEME, async (request) => {
    const file = request.method === "GET" ? resolveAppFile(source.dir, request.url) : null;
    if (file === null) {
      return notFound();
    }
    try {
      const body = await readFile(file);
      return new Response(body, {
        headers: {
          "Content-Type": contentType(file),
          "Content-Security-Policy": APP_CONTENT_SECURITY_POLICY,
          "X-Content-Type-Options": "nosniff",
          "Cache-Control": "no-cache",
        },
      });
    } catch {
      return notFound();
    }
  });
}

function notFound(): Response {
  return new Response("Not found", {
    status: 404,
    headers: { "Content-Type": "text/plain; charset=utf-8", "Content-Security-Policy": APP_CONTENT_SECURITY_POLICY },
  });
}
