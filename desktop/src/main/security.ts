/**
 * One place for the app's hardening (docs/architecture.md#security, process hardening): windows can't navigate, open windows,
 * attach webviews, or get any permission; nothing loads from the network; and every IPC message is
 * checked to come from the main frame of a window this process created, showing Unfold's own UI.
 */

import type { IpcMainEvent, IpcMainInvokeEvent, WebContents } from "electron";
import { app, session } from "electron";
import { APP_SCHEME, isUiUrl, type UiSource } from "./app-url";

/** Schemes a window may request besides the UI's own origin. */
const LOCAL_SCHEMES = new Set([`${APP_SCHEME}:`, "devtools:", "chrome-devtools:", "chrome-extension:", "data:", "blob:"]);

/** Applies every app-wide restriction. Call once, when the app is ready. */
export function hardenApp(source: UiSource): void {
  app.on("web-contents-created", (_event, contents) => {
    hardenContents(contents);
  });

  const ses = session.defaultSession;
  ses.setPermissionRequestHandler((_contents, _permission, callback) => {
    callback(false);
  });
  ses.setPermissionCheckHandler(() => false);
  ses.setDevicePermissionHandler(() => false);
  ses.on("select-hid-device", (event, _details, callback) => {
    event.preventDefault();
    callback();
  });
  ses.on("select-serial-port", (event, _ports, _contents, callback) => {
    event.preventDefault();
    callback("");
  });
  ses.on("select-usb-device", (event, _details, callback) => {
    event.preventDefault();
    callback();
  });
  ses.on("will-download", (event) => {
    event.preventDefault();
  });

  // Defense in depth beside the content security policy: nothing leaves the machine.
  ses.webRequest.onBeforeRequest((details, callback) => {
    callback({ cancel: !isLocalRequest(source, details.url) });
  });
}

function hardenContents(contents: WebContents): void {
  // Renderer-initiated navigation of any frame; the main process loads the UI with loadURL, which
  // does not pass through here.
  contents.on("will-frame-navigate", (event) => {
    event.preventDefault();
  });
  contents.on("will-redirect", (event) => {
    event.preventDefault();
  });
  contents.on("will-attach-webview", (event) => {
    event.preventDefault();
  });
  contents.setWindowOpenHandler(() => ({ action: "deny" }));
}

function isLocalRequest(source: UiSource, url: string): boolean {
  if (isUiUrl(source, url)) {
    return true;
  }
  if (source.kind === "dev") {
    // The dev server's hot-reload socket.
    const devSocket = new URL(source.url);
    devSocket.protocol = devSocket.protocol === "https:" ? "wss:" : "ws:";
    if (url.startsWith(devSocket.origin)) {
      return true;
    }
  }
  try {
    return LOCAL_SCHEMES.has(new URL(url).protocol);
  } catch {
    return false;
  }
}

/**
 * The sender check for every IPC message: it must come from a window's main frame, currently showing
 * the UI the main process loaded. Returns the sender's WebContents, or null if untrusted.
 */
export function trustedSender(source: UiSource, event: IpcMainEvent | IpcMainInvokeEvent): WebContents | null {
  const frame = event.senderFrame;
  if (!frame || frame !== event.sender.mainFrame || !isUiUrl(source, frame.url)) {
    return null;
  }
  return event.sender;
}
