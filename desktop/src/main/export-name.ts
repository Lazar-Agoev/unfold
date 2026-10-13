/**
 * The default file name the export save dialog offers. A pure function, tested without Electron.
 */

const MAX_STEM = 80;

/**
 * `<title>.png` for a board or page title: characters that file systems reject become `-`, and a
 * board ID (`.unfold/boards/order-placement.json`) gives its file name's stem.
 */
export function exportFileName(title: string | undefined): string {
  const text = title ?? "";
  const name = /\.json$/i.test(text) ? (text.split("/").at(-1) ?? "").slice(0, -".json".length) : text;
  const stem = name
    .replace(/[\\/:*?"<>|\p{Cc}]+/gu, "-")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/^[.-]+|[.-]+$/g, "")
    .slice(0, MAX_STEM)
    .trim();
  return `${stem || "board"}.png`;
}
