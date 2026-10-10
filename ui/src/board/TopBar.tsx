/**
 * The canvas top bar (components §5): the page's name and the board's publish state on the left, the
 * page's Code | Flow lens centered over the uncovered canvas (V-38, V-55), and Share and Publish on the
 * right. Share opens the share menu; Publish opens the publish dialog.
 */

import { Button, SegmentedControl, useContextMenu } from "../components";
import type { View } from "../ipc/generated";
import { shareMenuEntries } from "../menus";
import { findPage, setPageLens, useOpenBoard, usePageView, type PageTarget } from "../state/boards";
import { openDialog } from "../state/overlays";
import { usePublishState } from "./publishState";

const LENS_OPTIONS = [
  { value: "code", label: "Code" },
  { value: "flow", label: "Flow" },
] as const;

/** The share menu's width (V-43). */
const SHARE_MENU_WIDTH = 196;

export function TopBar({ target }: { readonly target: PageTarget }) {
  const title = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.title) ?? "";
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "code";
  const lens = usePageView(target, (view) => view.lens);
  const state = usePublishState(target.board);
  const share = useContextMenu("Share", SHARE_MENU_WIDTH);
  return (
    <header className="board-topbar">
      <h2 className="board-topbar-title">{title}</h2>
      <span className="board-publish-state">
        {state.dot !== null ? <span className={`board-publish-dot ${state.dot}`} aria-hidden="true" /> : null}
        {state.text}
      </span>
      <span className="board-topbar-spacer" />
      <SegmentedControl<View>
        className="board-lens"
        options={LENS_OPTIONS}
        value={lens ?? defaultView}
        onChange={(view) => {
          setPageLens(target, view);
        }}
        label="Lens"
        variant="lens"
      />
      <Button
        variant="secondary"
        trailingIcon="chevron-down"
        aria-haspopup="menu"
        onClick={(event) => {
          share.open(event.currentTarget, shareMenuEntries(target));
        }}
      >
        Share
      </Button>
      <Button
        variant="primary"
        disabled={!state.publishable}
        onClick={() => {
          openDialog({ kind: "publish", board: target.board });
        }}
      >
        Publish
      </Button>
      {share.element}
    </header>
  );
}
