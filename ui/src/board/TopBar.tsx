/**
 * The canvas top bar: the page's name on the left (and "Claude is working…" while an agent works on
 * the page), and the Show all calls switch on the right.
 */

import { Switch } from "../components";
import { Icon } from "../icons";
import { findPage, useOpenBoard, useWorkingAgents, workingText, type PageTarget } from "../state/boards";
import { setShowAllCalls, useShowAllCalls } from "../state/showAllCalls";

export function TopBar({ target }: { readonly target: PageTarget }) {
  const title = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.title) ?? "";
  const working = useWorkingAgents(target);
  const showAllCalls = useShowAllCalls();
  return (
    <header className="board-topbar">
      <div className="board-topbar-left">
        <h2 className="board-topbar-title">{title}</h2>
        {working.length > 0 ? (
          <span className="board-working" role="status">
            <Icon name="agent" size={10} />
            <span className="board-working-text">{workingText(working)}</span>
          </span>
        ) : null}
      </div>
      <Switch label="Show all calls" on={showAllCalls} onChange={setShowAllCalls} className="board-show-all-calls" />
    </header>
  );
}
