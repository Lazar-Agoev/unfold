/** Relative times for the recent-folders list: "just now", "2 hours ago", "yesterday", "Mar 4". */

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

function startOfDay(ms: number): number {
  const date = new Date(ms);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

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
