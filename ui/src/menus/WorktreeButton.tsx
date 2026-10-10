/**
 * The branch and worktree switcher on a project's Home header: the branch,
 * "· N worktrees" when there are several, and a popover in the menu style listing the repository's
 * worktrees from `git_info` (fetched each time it opens): branch, folder, a check on the current one,
 * unavailable ones disabled, and the note "Drafts and the graph are per worktree." Choosing one closes
 * this window's tabs of the project and switches it.
 */

import { useEffect, useRef, useState, type JSX, type KeyboardEvent } from "react";
import { Popover } from "../components";
import { Icon } from "../icons";
import { useEngineQuery, type GitInfo, type ProjectId, type ProjectRow } from "../ipc";
import { displayPath, plural } from "../lib/format";
import { useProjectRow } from "../state/workspace";
import { switchWorktree } from "./actions";
import "./menus.css";

type Worktree = GitInfo["worktrees"][number];

/** What a branch line says: the branch, or the first digits of a detached HEAD. */
function headText(head: { readonly branch: string | null; readonly detached: string | null }): string {
  return head.branch ?? (head.detached === null ? "no branch" : `detached ${head.detached}`);
}

function WorktreeList({ row, onDone }: { readonly row: ProjectRow; readonly onDone: () => void }) {
  const info = useEngineQuery("git_info", { project: row.id });
  const list = useRef<HTMLDivElement>(null);
  const worktrees = info.data?.worktrees ?? [];

  // Focus the current worktree once the list arrives.
  const loaded = info.data !== undefined;
  useEffect(() => {
    if (loaded) {
      (list.current?.querySelector<HTMLElement>("[aria-checked='true']") ?? list.current?.querySelector<HTMLElement>("[role=menuitemradio]"))?.focus();
    }
  }, [loaded]);

  const choose = (worktree: Worktree) => {
    onDone();
    if (!worktree.current) {
      void switchWorktree(row, worktree.path, headText(worktree));
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const items = [...(list.current?.querySelectorAll<HTMLElement>("[role=menuitemradio]:not([aria-disabled='true'])") ?? [])];
    const index = items.findIndex((item) => item === document.activeElement);
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (step !== 0 && items.length > 0) {
      event.preventDefault();
      items[(index + step + items.length) % items.length]?.focus();
    }
  };

  let body: JSX.Element | JSX.Element[];
  if (info.error) {
    body = <p className="worktree-message">{info.error.message}</p>;
  } else if (!info.data) {
    body = <p className="worktree-message">Reading worktrees…</p>;
  } else {
    body = worktrees.map((worktree) => (
      <div
        key={worktree.path}
        role="menuitemradio"
        aria-checked={worktree.current}
        aria-disabled={!worktree.available || undefined}
        tabIndex={-1}
        className="worktree-row"
        title={worktree.available ? worktree.path : `${worktree.path} isn't there any more.`}
        onClick={() => {
          if (worktree.available) {
            choose(worktree);
          }
        }}
        onKeyDown={(event) => {
          if ((event.key === "Enter" || event.key === " ") && worktree.available) {
            event.preventDefault();
            choose(worktree);
          }
        }}
      >
        <span className="worktree-check">{worktree.current ? <Icon name="check" /> : null}</span>
        <span className="worktree-branch">{headText(worktree)}</span>
        {/* The path's end tells worktrees apart: the start is elided. */}
        <span className="worktree-path">
          <bdi>{displayPath(worktree.path)}</bdi>
        </span>
      </div>
    ));
  }
  return (
    <div ref={list} role="menu" aria-label="Worktrees" className="worktree-list" onKeyDown={onKeyDown}>
      {body}
      <p className="worktree-note">Drafts and the graph are per worktree.</p>
    </div>
  );
}

/** The switcher for a project in a Git repository; nothing when it isn't in one. */
export function WorktreeButton({ project }: { readonly project: ProjectId }): JSX.Element {
  const row = useProjectRow(project);
  const [anchor, setAnchor] = useState<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const git = row?.git ?? null;
  if (!row || git === null) {
    return <></>;
  }
  return (
    <>
      <button
        ref={setAnchor}
        type="button"
        className="worktree-button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Branch ${headText(git)}. Switch worktree`}
        onClick={() => {
          setOpen((current) => !current);
        }}
      >
        <Icon name="branch" size={14} className="worktree-button-icon" />
        <span className="worktree-button-branch">{headText(git)}</span>
        {git.worktrees > 1 ? <span className="worktree-button-count">· {plural(git.worktrees, "worktree")}</span> : null}
        <Icon name="chevron-down-small" size={10} className="worktree-button-chevron" />
      </button>
      <Popover
        open={open}
        anchor={anchor}
        label="Switch worktree"
        width={264}
        onClose={() => {
          setOpen(false);
        }}
      >
        {open ? (
          <WorktreeList
            row={row}
            onDone={() => {
              setOpen(false);
            }}
          />
        ) : null}
      </Popover>
    </>
  );
}
