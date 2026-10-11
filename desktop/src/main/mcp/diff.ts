/**
 * The change between two versions of a small text file, as diff lines: what `update_project_settings`
 * returns for `unfold.toml` (design §8.1). Lines are matched by their longest common
 * subsequence; each change keeps a line of context around it, and `…` stands for the lines between.
 */

interface Line {
  readonly sign: " " | "-" | "+";
  readonly text: string;
}

function linesOf(text: string): string[] {
  if (text.length === 0) {
    return [];
  }
  const lines = text.split(/\r?\n/);
  return lines.at(-1) === "" ? lines.slice(0, -1) : lines;
}

/** Every line of both texts in order, marked kept, removed, or added. */
function compared(before: readonly string[], after: readonly string[]): Line[] {
  // common[i][j]: the longest common subsequence of before[i..] and after[j..].
  const common = Array.from({ length: before.length + 1 }, () => new Array<number>(after.length + 1).fill(0));
  for (let i = before.length - 1; i >= 0; i -= 1) {
    for (let j = after.length - 1; j >= 0; j -= 1) {
      const row = common[i] ?? [];
      row[j] = before[i] === after[j] ? (common[i + 1]?.[j + 1] ?? 0) + 1 : Math.max(common[i + 1]?.[j] ?? 0, row[j + 1] ?? 0);
    }
  }
  const lines: Line[] = [];
  let i = 0;
  let j = 0;
  while (i < before.length || j < after.length) {
    if (i < before.length && j < after.length && before[i] === after[j]) {
      lines.push({ sign: " ", text: before[i] ?? "" });
      i += 1;
      j += 1;
    } else if (j < after.length && (i === before.length || (common[i]?.[j + 1] ?? 0) > (common[i + 1]?.[j] ?? 0))) {
      // A tie removes first, so a changed line reads `-old` then `+new`.
      lines.push({ sign: "+", text: after[j] ?? "" });
      j += 1;
    } else {
      lines.push({ sign: "-", text: before[i] ?? "" });
      i += 1;
    }
  }
  return lines;
}

/**
 * The diff of `before` → `after` (`+` added, `-` removed, ` ` context), with `context` unchanged lines
 * around each change and `…` between hunks; empty when nothing changed.
 */
export function lineDiff(before: string, after: string, context = 1): string[] {
  const lines = compared(linesOf(before), linesOf(after));
  const near = (index: number) => lines.slice(Math.max(0, index - context), index + context + 1).some((line) => line.sign !== " ");
  const out: string[] = [];
  let skipped = false;
  lines.forEach((line, index) => {
    if (!near(index)) {
      skipped = true;
      return;
    }
    if (skipped && out.length > 0) {
      out.push("…");
    }
    skipped = false;
    out.push(`${line.sign}${line.text}`);
  });
  return out;
}
