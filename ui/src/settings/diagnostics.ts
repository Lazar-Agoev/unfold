/**
 * Copy Diagnostics (architecture §13): Rust's redacted summary (versions, platform, counts; no
 * source, no paths), with the app, Electron, and Chromium versions only the window knows, put on
 * the clipboard by the main process (windows have no clipboard permission).
 */

import { showToast } from "../components";
import { call, toUnfoldError, windowCommand, type ProjectId } from "../ipc";

/** The app, Electron, and Chromium versions from the window's user agent, one per line. */
export function shellVersions(userAgent: string): string {
  const app = /\(KHTML, like Gecko\) (\S+)\/(\S+) Chrome\//.exec(userAgent);
  const electron = /\bElectron\/(\S+)/.exec(userAgent);
  const chromium = /\bChrome\/(\S+)/.exec(userAgent);
  return [
    `App: ${app?.[1] !== undefined && app[2] !== undefined ? `${app[1]} ${app[2]}` : "unknown"}`,
    `Electron: ${electron?.[1] ?? "unknown"}`,
    `Chromium: ${chromium?.[1] ?? "unknown"}`,
  ].join("\n");
}

/** The whole report: Rust's summary, then the shell's versions. */
export function diagnosticsText(rust: string, userAgent: string): string {
  const [title = "", ...rest] = rust.trimEnd().split("\n");
  return [title, shellVersions(userAgent), ...rest].join("\n") + "\n";
}

/** Builds the report (describing `project` as well, when given) and copies it, then says so. */
export async function copyDiagnostics(project?: ProjectId): Promise<void> {
  try {
    const report = await call("diagnostics", project === undefined ? {} : { project });
    await windowCommand("copy_to_clipboard", { text: diagnosticsText(report.text, navigator.userAgent) });
    showToast({ title: "Copied diagnostics", body: "They hold versions and counts: no code and no paths." });
  } catch (error) {
    showToast({ title: "Couldn't copy diagnostics", body: toUnfoldError(error).message, tone: "danger" });
  }
}
