/** Make a group from lines: the typed line range ("27-31", "27–31", or one line "27"). */

/** `[27, 31]` for "27-31" (either order), `[27, 27]` for "27"; null when it isn't a line range. */
export function parseLines(text: string): [number, number] | null {
  const match = /^\s*(\d+)\s*(?:[-–—:]\s*(\d+))?\s*$/.exec(text);
  if (!match?.[1]) {
    return null;
  }
  const from = Number(match[1]);
  const to = match[2] !== undefined ? Number(match[2]) : from;
  return [Math.min(from, to), Math.max(from, to)];
}
