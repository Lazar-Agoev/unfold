/**
 * Board cards in a responsive grid: a thumbnail of the first page, the
 * title, and a subline. Click opens the board; the right-click menu is the board menu. Long grids
 * render only the rows of cards in view, and fetch thumbnails only for those.
 */

import { useLayoutEffect, useState, type MouseEvent } from "react";
import { Monogram, useContextMenu } from "../../components";
import type { ProjectId, Thumbnail } from "../../ipc";
import { boardMenuEntries } from "../../menus";
import { openBoardFromHome } from "../actions";
import { useWindowed } from "../windowed";
import { THUMB_HEIGHT, THUMB_WIDTH, thumbnailDrawing } from "./thumbnail";
import { useThumbnails } from "./useThumbnails";

/** What a board card shows. */
export interface BoardCardModel {
  readonly project: ProjectId;
  readonly id: string;
  readonly title: string;
  readonly draft: boolean;
  /** "3 pages · Published · edited 2 hours ago". */
  readonly subline: string;
  /** Other projects its cards come from: "· [W] with storefront-web". */
  readonly with: readonly string[];
  /** The board's own project's name, shown first on cross-project lists. */
  readonly projectName?: string;
}

const MIN_COLUMN = 220;
const GAP = 16;
/** Card height: 140 thumbnail, 10 gap, 20 + 2 + 16 meta. */
const CARD_HEIGHT = 188;
/** Past this many boards the grid renders only the rows in view. */
const WINDOW_THRESHOLD = 24;

function ThumbnailDrawing({ thumbnail }: { readonly thumbnail: Thumbnail }) {
  if (thumbnail.error !== null) {
    return <span className="home-thumb-note">Can't be read</span>;
  }
  const drawing = thumbnailDrawing(thumbnail);
  return (
    <svg className="home-thumb-svg" width={THUMB_WIDTH} height={THUMB_HEIGHT} viewBox={`0 0 ${THUMB_WIDTH} ${THUMB_HEIGHT}`} aria-hidden="true">
      <path className="home-thumb-edge" d={drawing.edges} />
      <path className="home-thumb-link" d={drawing.links} />
      {drawing.cards.map((card) => (
        <g key={card.id}>
          <rect className="home-thumb-card" x={card.x} y={card.y} width={card.width} height={card.height} rx={5} />
          {card.monogram ? <rect className="home-thumb-mono" x={card.monogram.x} y={card.monogram.y} width={9} height={9} rx={2.5} /> : null}
          <path className="home-thumb-line" d={card.lines.filter((line) => !line.accent).map((line) => `M${line.x1} ${line.y}H${line.x2}`).join("")} />
          {card.lines.some((line) => line.accent) ? (
            <path className="home-thumb-accent" d={card.lines.filter((line) => line.accent).map((line) => `M${line.x1} ${line.y}H${line.x2}`).join("")} />
          ) : null}
        </g>
      ))}
    </svg>
  );
}

function BoardCard({ board, thumbnail, onMenu }: { readonly board: BoardCardModel; readonly thumbnail: Thumbnail | undefined; readonly onMenu: (event: MouseEvent) => void }) {
  const ref = { project: board.project, id: board.id };
  return (
    <div
      role="button"
      tabIndex={0}
      className="home-board-card"
      data-board-key={`${board.project}\u0000${board.id}`}
      aria-label={board.title}
      onClick={() => void openBoardFromHome(ref)}
      onKeyDown={(event) => {
        if (event.key === "Enter" && event.target === event.currentTarget) {
          event.preventDefault();
          void openBoardFromHome(ref);
        }
      }}
      onContextMenu={onMenu}
    >
      <div className={board.draft ? "home-thumb home-thumb-draft" : "home-thumb"}>{thumbnail ? <ThumbnailDrawing thumbnail={thumbnail} /> : null}</div>
      <div className="home-board-meta">
        <span className="home-board-title">{board.title}</span>
        <span className="home-board-subline">
          {board.projectName === undefined ? null : (
            <>
              <Monogram name={board.projectName} size={14} />
              <span>{board.projectName} ·</span>
            </>
          )}
          <span className="home-board-words">{board.subline}</span>
          {board.with.length > 0 ? (
            <>
              <span>·</span>
              <Monogram name={board.with[0] ?? ""} size={14} />
              <span className="home-board-words">with {board.with.join(", ")}</span>
            </>
          ) : null}
        </span>
      </div>
    </div>
  );
}

/** How many columns of at least 220px fit (four at full width). */
function useColumns(element: HTMLElement | null): number {
  const [columns, setColumns] = useState(4);
  useLayoutEffect(() => {
    if (!element) {
      return undefined;
    }
    const measure = () => {
      const width = element.clientWidth;
      if (width > 0) {
        setColumns(Math.max(1, Math.floor((width + GAP) / (MIN_COLUMN + GAP))));
      }
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [element]);
  return columns;
}

export function BoardGrid({ boards, label }: { readonly boards: readonly BoardCardModel[]; readonly label: string }) {
  const menu = useContextMenu("Board menu", 220);
  const [grid, setGrid] = useState<HTMLDivElement | null>(null);
  const columns = useColumns(grid);
  const rows = Math.ceil(boards.length / columns);
  const { attach, rows: windowRows, height: windowHeight, offsets, range } = useWindowed({ count: rows, size: () => CARD_HEIGHT + GAP, threshold: boards.length > WINDOW_THRESHOLD ? 0 : Number.POSITIVE_INFINITY });
  const [firstRow, endRow] = range;
  const thumbnailOf = useThumbnails(boards, [firstRow * columns, endRow * columns]);

  const card = (board: BoardCardModel) => (
    <BoardCard
      key={`${board.project}\u0000${board.id}`}
      board={board}
      thumbnail={thumbnailOf(board)}
      onMenu={(event) => {
        menu.open(event, boardMenuEntries({ project: board.project, id: board.id }));
      }}
    />
  );

  return (
    <div
      ref={(element) => {
        setGrid(element);
        attach(element);
      }}
      role="group"
      aria-label={label}
      className={windowRows ? "home-board-grid home-board-grid-windowed" : "home-board-grid"}
      style={windowRows ? { height: windowHeight } : undefined}
    >
      {windowRows
        ? windowRows.map((row, slot) => (
            <div key={row.index} className="home-board-row" style={{ transform: `translateY(${offsets[slot] ?? 0}px)`, gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
              {boards.slice(row.index * columns, (row.index + 1) * columns).map(card)}
            </div>
          ))
        : boards.map(card)}
      {menu.element}
    </div>
  );
}
