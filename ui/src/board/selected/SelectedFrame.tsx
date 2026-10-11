/**
 * Selected for a frame: its title (renamed in place, which makes it yours), its look and where it is
 * nested, who drew it, its description, what it holds (each brought into view by a click), the lines
 * in and out (drawn links today; typed connections are links too, so they list here as well), the
 * repairs its health offers, Collapse or Expand, and Remove frame (its members stay).
 */

import { Author } from "../../agent";
import { frameDescriptionField } from "../../canvas/frames/frameMenu";
import { Button, TextButton, showToast } from "../../components";
import type { Frame, Link, Page } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import { useAuthor } from "../../state/author";
import {
  editFrame,
  findCard,
  findFrame,
  findPage,
  focusCard,
  focusFrame,
  focusItem,
  frameItems,
  frameOf,
  itemsFor,
  removeFrame,
  repairBoard,
  repairEffect,
  repairLabel,
  repairSpec,
  select,
  useOpenBoard,
  type PageTarget,
} from "../../state/boards";
import { AuthoredField } from "./AuthoredField";
import { itemName } from "./ClaimRows";
import { RelationRow } from "./Relations";

const LOOK_WORDS = { box: "Box", zone: "Zone" } as const;

/** Brings an item of the page into view and selects it: a frame, a card, a resource card, or a note. */
function showItem(target: PageTarget, page: Page, id: string): void {
  if (findFrame(page, id)) {
    focusFrame(target, id);
  } else if (findCard(page, id)) {
    focusCard(target, id);
  } else {
    focusItem(target, id);
  }
}

/** The links that cross the frame's border: out of it (from inside, to outside) and into it. */
function crossing(page: Page, frame: Frame): { readonly link: Link; readonly out: boolean }[] {
  const inside = new Set([frame.id, ...frameItems(page, frame.id)]);
  return page.links.flatMap((link) => (inside.has(link.from) === inside.has(link.to) ? [] : [{ link, out: inside.has(link.from) }]));
}

export function SelectedFrame({ target, frame }: { readonly target: PageTarget; readonly frame: Frame }) {
  const author = useAuthor();
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const health = useOpenBoard(target.board, (board) => board.health);
  if (!page) {
    return null;
  }
  const issues = itemsFor(health, target.page, { frame: frame.id });
  const holder = frameOf(page, frame.id);
  const lines = crossing(page, frame);
  const collapsed = frame.collapsed === true;
  const repair = async (item: (typeof issues)[number], kind: (typeof issues)[number]["repairs"][number]) => {
    const spec = repairSpec(kind, item, page);
    if (!spec) {
      return;
    }
    try {
      await repairBoard(target.board, spec);
    } catch (error) {
      showToast({ title: "Couldn't repair the board", body: toUnfoldError(error).message, tone: "danger" });
    }
  };
  return (
    <>
      <div className="selected-identity">
        <input
          className="selected-name ui selected-title-input"
          aria-label="Frame title"
          value={frame.title}
          onChange={(event) => {
            if (event.target.value.trim().length > 0 && author !== null) {
              editFrame(target, frame.id, { title: event.target.value, author });
            }
          }}
        />
        <p className="selected-location">
          {LOOK_WORDS[frame.look]} frame
          {holder ? (
            <>
              {" in "}
              <button
                type="button"
                className="selected-link"
                onClick={() => {
                  select(target, { kind: "frame", frameId: holder.id });
                }}
              >
                {holder.title}
              </button>
            </>
          ) : null}
          {" · drawn by "}
          <Author author={frame.author} />
        </p>
      </div>
      <AuthoredField
        label="Frame description"
        value={frame.description}
        placeholder="What it does, in one sentence…"
        focusKey={frameDescriptionField(frame.id)}
        disabledReason={author === null ? "Loading who you are…" : null}
        status={issues.map((item) => (
          <span key={item.message} className="authored-note">
            {item.message}{" "}
            {item.repairs.map((kind) => (
              <TextButton key={kind} title={repairEffect(kind, item, page)} onClick={() => void repair(item, kind)}>
                {repairLabel(kind)}
              </TextButton>
            ))}
          </span>
        ))}
        onChange={(text) => {
          if (author !== null) {
            editFrame(target, frame.id, { description: { text, author } });
          }
        }}
      />
      <div className="selected-relations">
        {frame.members.map((id, index) => (
          <RelationRow
            key={id}
            {...(index === 0 ? { label: "Holds" } : {})}
            onActivate={() => {
              showItem(target, page, id);
            }}
          >
            <span className="relation-code">{itemName(page, id)}</span>
          </RelationRow>
        ))}
        {lines.map(({ link, out }, index) => (
          <RelationRow
            key={link.id}
            {...(index === 0 ? { label: "Lines" } : {})}
            onActivate={() => {
              select(target, { kind: "link", linkId: link.id });
            }}
          >
            <span className="relation-code">
              {out ? "→ " : "← "}
              {itemName(page, out ? link.to : link.from)}
            </span>
            {link.label ? <span className="relation-path">{` · ${link.label.text}`}</span> : null}
          </RelationRow>
        ))}
      </div>
      <div className="selected-actions">
        <Button
          variant="secondary"
          onClick={() => {
            editFrame(target, frame.id, { collapsed: !collapsed });
          }}
        >
          {collapsed ? "Expand" : "Collapse"}
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            removeFrame(target, frame.id);
          }}
        >
          Remove frame
        </Button>
      </div>
    </>
  );
}
