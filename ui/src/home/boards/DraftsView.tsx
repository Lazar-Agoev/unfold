/**
 * Drafts: the drafts (with published boards that have unpublished changes) of every project, as a board
 * grid in Home's main area. Each card names its project; the window's selected project stays as it was.
 */

import { useMemo, type ReactNode } from "react";
import { EmptyState } from "../../components";
import { useEngineQuery } from "../../ipc";
import { formatCount } from "../../lib/format";
import { useProjectIndex } from "../../state/workspace";
import { BoardGrid } from "./BoardGrid";
import { cardOf } from "./BoardsSection";
import "./boards.css";

/** Drafts lists at most this many. */
const DRAFTS_CAP = 500;

function ViewPage({ title, note, children }: { readonly title: string; readonly note: string; readonly children: ReactNode }) {
  return (
    <div className="home-page">
      <header className="home-header">
        <h1 className="home-title">{title}</h1>
        <p className="home-view-note">{note}</p>
      </header>
      {children}
    </div>
  );
}

export function DraftsView() {
  const drafts = useEngineQuery("drafts", {});
  const index = useProjectIndex();
  const boards = useMemo(
    () => (drafts.data?.drafts ?? []).map((board) => cardOf(board, index.byId.get(board.project)?.name ?? "")),
    [drafts.data, index],
  );
  const count = drafts.data?.drafts.length ?? 0;
  const note =
    count >= DRAFTS_CAP
      ? `The ${formatCount(DRAFTS_CAP)} most recent drafts and boards with unpublished changes, in every project.`
      : "Drafts and boards with unpublished changes, in every project, most recently edited first.";
  let body: ReactNode;
  if (drafts.error) {
    body = <p className="home-boards-note">{drafts.error.message}</p>;
  } else if (drafts.data && boards.length === 0) {
    body = <EmptyState title="No drafts" body="A new board starts as a draft, kept here until you publish it." />;
  } else {
    body = <BoardGrid boards={boards} label="Drafts" />;
  }
  return (
    <ViewPage title="Drafts" note={note}>
      {body}
    </ViewPage>
  );
}
