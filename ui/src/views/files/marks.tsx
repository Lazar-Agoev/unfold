/**
 * The Files view's quiet marks (architecture §11.3): `→N` entrances, `●` on the open board, an amber
 * dot for a changed or pending file, a warning for a file that doesn't parse, "not indexed",
 * "ignored", and "N launches".
 */

import type { ReactNode } from "react";
import { CountMark, DotMark, TextMark, WarningMark } from "../../components";
import type { DirEntry, OutlineEntry } from "../../ipc/generated";
import { plural } from "../../shell/status";
import { IGNORED, NOT_INDEXED, SKIP_REASONS } from "./model";

/** Whether any key on the board belongs to the file (its root or anything inside it). */
function fileOnBoard(path: string, onBoard: ReadonlySet<string>): boolean {
  const prefix = `${path}#`;
  for (const key of onBoard) {
    if (key === path || key.startsWith(prefix)) {
      return true;
    }
  }
  return false;
}

function defOnBoard(def: OutlineEntry, onBoard: ReadonlySet<string>): boolean {
  return onBoard.has(def.key);
}

export function entryMarks(entry: DirEntry, { onBoard, expanded }: { onBoard: ReadonlySet<string>; expanded: boolean }): ReactNode {
  const marks: ReactNode[] = [];
  if (entry.state === "last_good") {
    marks.push(<WarningMark key="parse" label="Doesn't parse · showing its last version that parsed" />);
  } else if (entry.state === "failed") {
    marks.push(<WarningMark key="parse" label="Doesn't parse" />);
  }
  if (entry.pending) {
    marks.push(
      <DotMark key="pending" tone="pending" label={entry.kind === "folder" ? "Something inside changed since the graph" : "Changed since the graph"} />,
    );
  }
  if (entry.skipped !== null && NOT_INDEXED.has(entry.skipped)) {
    marks.push(
      <span key="skipped" title={SKIP_REASONS[entry.skipped]}>
        <TextMark>not indexed</TextMark>
      </span>,
    );
  } else if (entry.skipped !== null && IGNORED.has(entry.skipped)) {
    marks.push(
      <span key="skipped" title={SKIP_REASONS[entry.skipped]}>
        <TextMark>ignored</TextMark>
      </span>,
    );
  }
  if (entry.launches > 0) {
    marks.push(<TextMark key="launches">{plural(entry.launches, "launch", "launches")}</TextMark>);
  }
  // A collapsed file shows that something in it is on the board; expanded, its definitions do.
  if (entry.kind === "file" && !expanded && fileOnBoard(entry.path, onBoard)) {
    marks.push(<DotMark key="board" tone="accent" label="On the open board" />);
  }
  if (entry.entrances > 0) {
    marks.push(<CountMark key="entrances" count={entry.entrances} label={plural(entry.entrances, "entrance")} />);
  }
  return marks.length > 0 ? marks : undefined;
}

export function defMarks(def: OutlineEntry, onBoard: ReadonlySet<string>): ReactNode {
  return defOnBoard(def, onBoard) ? <DotMark tone="accent" label="On the open board" /> : undefined;
}
