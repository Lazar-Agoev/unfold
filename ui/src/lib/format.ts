/** Words and numbers as the chrome shows them: counts, plurals, languages, paths, and times. */

import type { Language } from "../ipc/generated";

/** `1,284`. With `max`, a count at or past the cap reads `500+`. */
export function formatCount(count: number, max?: number): string {
  if (max !== undefined && count >= max) {
    return `${max.toLocaleString("en-US")}+`;
  }
  return count.toLocaleString("en-US");
}

/** `1 file`, `1,058 files`; `many` for irregular plurals. */
export function plural(count: number, one: string, many = `${one}s`): string {
  return `${formatCount(count)} ${count === 1 ? one : many}`;
}

const LANGUAGE_NAMES: Readonly<Record<Language, string>> = { typescript: "TypeScript", python: "Python" };

/** `TypeScript + Python`. */
export function languageList(languages: readonly Language[]): string {
  return languages.map((language) => LANGUAGE_NAMES[language]).join(" + ");
}

/** A home folder at the start of an absolute path: `/Users/<name>`, `/home/<name>`, `C:\Users\<name>`. */
const HOME = /^(?:\/Users\/[^/]+|\/home\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)(?=$|[/\\])/;

/** An absolute path as people read it, with the home folder as `~`: `~/code/storefront-api`. */
export function displayPath(path: string): string {
  return path.replace(HOME, "~");
}

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

function startOfDay(ms: number): number {
  const date = new Date(ms);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** "just now", "5 minutes ago", "2 hours ago", "yesterday", "3 days ago", "Mar 4", "Mar 4, 2024". */
export function relativeTime(atMs: number, nowMs: number = Date.now()): string {
  const elapsed = Math.max(0, nowMs - atMs);
  if (elapsed < MINUTE) {
    return "just now";
  }
  if (elapsed < HOUR) {
    const minutes = Math.floor(elapsed / MINUTE);
    return minutes === 1 ? "a minute ago" : `${minutes} minutes ago`;
  }
  const days = Math.round((startOfDay(nowMs) - startOfDay(atMs)) / (24 * HOUR));
  if (days === 0) {
    const hours = Math.floor(elapsed / HOUR);
    return hours === 1 ? "an hour ago" : `${hours} hours ago`;
  }
  if (days === 1) {
    return "yesterday";
  }
  if (days < 7) {
    return `${days} days ago`;
  }
  const date = new Date(atMs);
  const sameYear = date.getFullYear() === new Date(nowMs).getFullYear();
  return date.toLocaleDateString("en-US", sameYear ? { month: "short", day: "numeric" } : { month: "short", day: "numeric", year: "numeric" });
}
