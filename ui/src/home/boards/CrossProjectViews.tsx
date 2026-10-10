/**
 * Recents and Drafts: the boards opened lately and the drafts (with published boards that have
 * unpublished changes) of every project, as board grids in Home's main area. Each card names its
 * project; the window's selected project stays as it was.
 */

import { useMemo, type ReactNode } from "react";
import { EmptyState } from "../../components";
import { useEngineQuery, type RecentBoard, type Thumbnail } from "../../ipc";
import { formatCount, plural, relativeTime } from "../../lib/format";
import { useProjectIndex, useWorkspace } from "../../state/workspace";
import { BoardGrid, type BoardCardModel } from "./BoardGrid";
import { cardOf } from "./BoardsSection";
import { isDraftId } from "./model";
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

/** A recent board as a card: its pages come with the thumbnail. */
function recentCard(recent: RecentBoard, projectName: string): BoardCardModel {
  const draft = isDraftId(recent.board);
  const words = `${draft ? "Draft" : "Published"} · opened ${relativeTime(recent.openedAtMs)}`;
  return {
    project: recent.project,
    id: recent.board,
    title: recent.title,
    draft,
    subline: (thumbnail: Thumbnail | undefined) => (thumbnail ? `${plural(thumbnail.pages, "page")} · ${words}` : words),
    with: [],
    projectName,
  };
}

export function RecentsView() {
  const workspace = useWorkspace();
  const index = useProjectIndex();
  const boards = useMemo(
    () =>
      (workspace?.recentBoards ?? []).flatMap((recent) => {
        const project = index.byId.get(recent.project);
        return project ? [recentCard(recent, project.name)] : [];
      }),
    [workspace, index],
  );
  return (
    <ViewPage title="Recents" note="Boards you opened lately, in every project.">
      {workspace && boards.length === 0 ? (
        <EmptyState title="No recent boards" body="Boards you open show up here." />
      ) : (
        <BoardGrid boards={boards} label="Recent boards" />
      )}
    </ViewPage>
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
