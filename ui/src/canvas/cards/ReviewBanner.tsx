/**
 * Health on a card or a resource card: a full-width banner under the header ("Changed since it was
 * described · Review"), and the menu its Review opens (core-scope.html's A CARD TO REVIEW): Accept,
 * Reconnect…, and Remove from page (`reviewRepairs`), each explained in its tooltip (`repairEffect`)
 * and saved at once as one undo step. Reconnect… first asks for the definition
 * (or app or entrance) the reference became by a name search, offering the one the engine found when
 * exactly one fits; for a proposed step, the block of a card drawing it that it goes after now (an
 * explicit placement repair, never by a similar name). A card's banner also covers the combined and
 * proposed steps it draws. Selected and the code panel open the same menu (`ReviewButton`).
 */

import { useEffect, useMemo, useState } from "react";
import { Button, Popover, SearchField, TextButton, showToast, useContextMenu, type MenuEntry } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { CardView, HealthItem, HealthKind, Key, Page, ProjectId, RepairKind, Target } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import { findCard, findPage, getOpenBoard, itemsFor, kindWord, repairBoard, repairEffect, repairLabel, repairSpec, useOpenBoard, type PageTarget } from "../../state/boards";
import { cardsDrawing, fetchCard, flowBlocks, overlayIds, type DrawnBlock } from "../../state/cards";

/** What the banner says about a card's first problem. */
const BANNER_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Changed since it was described",
  outdated: "Changed since it was described",
  missing: "Missing from the graph",
  brokenCombined: "A combined step no longer matches the code",
  brokenLink: "A connection lost its other end",
};

/** What the banner says about a claim's (a resource card's, a connection's) first problem. */
const CLAIM_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Changed since it was drawn",
  outdated: "Changed since it was drawn",
  missing: "Cites code that is gone",
  brokenCombined: "A combined step no longer matches the code",
  brokenLink: "Lost an end or what it cites",
};

/** What a card's banner says when its first problem is about one of its proposed steps. */
const PROPOSAL_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Where a proposed step goes needs review",
  outdated: "Where a proposed step goes needs review",
  missing: "A proposed step lost the step it goes after",
  brokenCombined: "A combined step no longer matches the code",
  brokenLink: "A connection lost its other end",
};

/** What a claim's review mark says about its first problem (a connection's chip, Selected). */
export function claimProblem(item: HealthItem): string {
  return CLAIM_TEXT[item.kind];
}

const NO_ITEMS: readonly HealthItem[] = [];

/** The health items about a card and the combined and proposed steps it draws (`view`), on its page. */
function useCardHealth(target: PageTarget, cardId: string, view: CardView | undefined): readonly HealthItem[] {
  const health = useOpenBoard(target.board, (board) => board.health);
  return useMemo(() => {
    const drawn = new Set(view ? overlayIds(view) : []);
    const items = (health?.items ?? []).filter(
      (item) =>
        item.page === target.page &&
        (("card" in item.target && item.target.card === cardId) ||
          ("combined" in item.target && drawn.has(item.target.combined)) ||
          ("proposal" in item.target && drawn.has(item.target.proposal))),
    );
    return items.length > 0 ? items : NO_ITEMS;
  }, [health, view, target.page, cardId]);
}

/** The health items about a claim (a resource card or a link) on its page. */
function useClaimHealth(target: PageTarget, item: Target): readonly HealthItem[] {
  const health = useOpenBoard(target.board, (board) => board.health);
  const key = JSON.stringify(item);
  return useMemo(() => {
    const items = itemsFor(health, target.page, JSON.parse(key) as Target);
    return items.length > 0 ? items : NO_ITEMS;
  }, [health, target.page, key]);
}

function ReconnectPicker({ project, entrance, value, onChange }: { readonly project: ProjectId; readonly entrance: boolean; readonly value: string; readonly onChange: (key: string) => void }) {
  const [query, setQuery] = useState("");
  const results = useEngineQuery(
    "search",
    { query, kinds: [entrance ? "entrances" : "definitions"], limit: 8, projects: [project] },
    { enabled: query.trim().length > 0 },
  );
  const hits = (entrance ? results.data?.entrances.hits : results.data?.definitions.hits) ?? [];
  return (
    <div className="repair-reconnect">
      <SearchField value={query} onChange={setQuery} label={entrance ? "Find the entrance" : "Find the definition it became"} placeholder={entrance ? "Find the entrance…" : "Find the definition it became…"} />
      <div role="listbox" aria-label="Reconnect to" className="repair-hits">
        {hits.map((hit) => (
          <button
            key={hit.key}
            type="button"
            role="option"
            aria-selected={value === hit.key}
            className="repair-hit"
            onClick={() => {
              onChange(hit.key);
            }}
          >
            <span className="repair-hit-name">{hit.qualifiedName || hit.name}</span>
            <span className="repair-hit-path">
              {hit.path}
              {hit.line !== null ? `:${hit.line}` : ""}
            </span>
          </button>
        ))}
        {query.trim().length > 0 && results.isSuccess && hits.length === 0 ? <p className="repair-note">Nothing matches.</p> : null}
      </div>
    </div>
  );
}

/** The blocks a proposed step can go after now: those of the cards drawing it that take one, but the one it goes after. */
function useProposalPlaces(target: PageTarget, proposalId: string, after: Key | undefined): { readonly places: readonly DrawnBlock[]; readonly loading: boolean } {
  const [state, setState] = useState<{ readonly places: readonly DrawnBlock[]; readonly loading: boolean }>({ places: [], loading: true });
  useEffect(() => {
    let live = true;
    void Promise.all(cardsDrawing(target, proposalId).map((cardId) => fetchCard(target, cardId).catch(() => undefined))).then((views) => {
      if (!live) {
        return;
      }
      const blocks = views.flatMap((view) => (view?.type === "definition" ? flowBlocks(view.flow) : []));
      const places = blocks.filter((block, index) => block.actions?.after === true && block.key !== after && blocks.findIndex((other) => other.key === block.key) === index);
      setState({ places, loading: false });
    });
    return () => {
      live = false;
    };
  }, [target, proposalId, after]);
  return state;
}

/** Reconnect for a proposed step: the blocks of the cards drawing it it can go after now. */
function StepPicker({ target, item, value, onChange }: { readonly target: PageTarget; readonly item: HealthItem; readonly value: Key; readonly onChange: (key: Key) => void }) {
  const proposalId = "proposal" in item.target ? item.target.proposal : "";
  const { places, loading } = useProposalPlaces(target, proposalId, item.key);
  return (
    <div className="repair-reconnect">
      <div role="listbox" aria-label="Goes after" className="repair-hits">
        {places.map((place) => (
          <button
            key={place.key}
            type="button"
            role="option"
            aria-selected={value === place.key}
            className="repair-hit"
            onClick={() => {
              onChange(place.key);
            }}
          >
            <span className="repair-hit-name">{place.name ?? place.label}</span>
          </button>
        ))}
        {loading ? <p className="repair-note">Loading its blocks…</p> : null}
        {!loading && places.length === 0 ? <p className="repair-note">No card on this page draws a block it can follow right now.</p> : null}
      </div>
    </div>
  );
}

/** Reconnect…: the block (or definition, app, or entrance) a problem's reference became, then Reconnect, saved at once. */
function ReconnectPopover({
  target,
  project,
  item,
  anchor,
  onClose,
}: {
  readonly target: PageTarget;
  readonly project: ProjectId;
  readonly item: HealthItem;
  readonly anchor: HTMLElement;
  readonly onClose: () => void;
}) {
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  const [to, setTo] = useState(item.reconnect ?? "");
  const [busy, setBusy] = useState(false);
  const spec = page ? repairSpec("reconnect", item, page, to) : null;
  // A card's own reference is reconnected to one of its kind; anything else names a block.
  const about = page && "card" in item.target && item.key === undefined ? findCard(page, item.target.card) : undefined;
  const entrance = about !== undefined && "entrance" in about.ref;
  const apply = async () => {
    if (!spec) {
      return;
    }
    setBusy(true);
    try {
      await repairBoard(target.board, spec);
      onClose();
    } catch (error) {
      showToast({ title: "Couldn't repair the board", body: toUnfoldError(error).message, tone: "danger" });
      setBusy(false);
    }
  };
  return (
    <Popover open anchor={anchor} label="Reconnect" onClose={onClose} side="bottom" align="end" width={340} className="repair-popover">
      <p className="repair-message">{page ? repairEffect("reconnect", item, page) : item.message}</p>
      {"proposal" in item.target ? (
        <StepPicker target={target} item={item} value={to} onChange={setTo} />
      ) : (
        <ReconnectPicker project={project} entrance={entrance} value={to} onChange={setTo} />
      )}
      <div className="repair-actions">
        <Button variant="secondary" size="sm" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="primary" size="sm" busy={busy} disabled={!spec} onClick={() => void apply()}>
          Reconnect
        </Button>
      </div>
    </Popover>
  );
}

/**
 * What the review menu offers for an item, in order (core-scope.html's A CARD TO REVIEW): Accept (the
 * accept or re-sign repair, whichever the item offers), Reconnect…, and Remove from page. A card can be
 * pointed at another block or removed whatever its problem; removing only its description is left to
 * Selected and agents.
 */
function reviewRepairs(item: HealthItem): RepairKind[] {
  const card = "card" in item.target;
  const accept = item.repairs.find((kind) => kind === "accept" || kind === "resign");
  return [
    ...(accept ? [accept] : []),
    ...(card || item.repairs.includes("reconnect") ? (["reconnect"] as const) : []),
    ...(card || item.repairs.includes("remove") ? (["remove"] as const) : []),
  ];
}

/**
 * The review menu's entries for `items`: what each problem offers (`reviewRepairs`), explained in their tooltips, removals
 * last in the danger color. Several problems are headed by what each is. Reconnect… asks for the new
 * block first (`onReconnect`); every other repair is saved at once.
 */
function reviewEntries(target: PageTarget, page: Page, items: readonly HealthItem[], onReconnect: (item: HealthItem) => void): MenuEntry[] {
  return items.flatMap((item, index): MenuEntry[] => {
    const entry = (kind: RepairKind): MenuEntry => ({
      id: `${index}:${kind}`,
      label: repairLabel(kind),
      hint: repairEffect(kind, item, page),
      ...(kind === "remove" ? { danger: true } : {}),
      onSelect: () => {
        if (kind === "reconnect") {
          onReconnect(item);
          return;
        }
        const spec = repairSpec(kind, item, page);
        if (spec) {
          repairBoard(target.board, spec).catch((error: unknown) => {
            showToast({ title: "Couldn't repair the board", body: toUnfoldError(error).message, tone: "danger" });
          });
        }
      },
    });
    const repairs = reviewRepairs(item);
    const keep = repairs.filter((kind) => kind !== "remove").map(entry);
    const remove = repairs.includes("remove") ? [entry("remove")] : [];
    return [
      ...(index > 0 ? [{ kind: "separator" as const, id: `${index}:rule` }] : []),
      ...(items.length > 1 ? [{ kind: "heading" as const, id: `${index}:heading`, label: kindWord(item.kind) }] : []),
      ...keep,
      ...(keep.length > 0 && remove.length > 0 ? [{ kind: "separator" as const, id: `${index}:remove` }] : []),
      ...remove,
    ];
  });
}

/** "Review", opening the review menu of `items` (nothing when there are none): Accept, Reconnect…, Remove from page, as each allows. */
export function ReviewButton({ target, project, items }: { readonly target: PageTarget; readonly project: ProjectId; readonly items: readonly HealthItem[] }) {
  const menu = useContextMenu("Review", 220);
  const [reconnecting, setReconnecting] = useState<{ readonly item: HealthItem; readonly anchor: HTMLElement } | null>(null);
  if (items.length === 0) {
    return null;
  }
  return (
    <>
      <TextButton
        className="card-review-action"
        aria-haspopup="menu"
        onClick={(event) => {
          event.stopPropagation();
          const button = event.currentTarget;
          const board = getOpenBoard(target.board);
          const page = board ? findPage(board.document, target.page) : undefined;
          if (page) {
            menu.open(
              button,
              reviewEntries(target, page, items, (item) => {
                setReconnecting({ item, anchor: button });
              }),
            );
          }
        }}
      >
        Review
      </TextButton>
      {menu.element}
      {reconnecting ? (
        <ReconnectPopover
          target={target}
          project={project}
          item={reconnecting.item}
          anchor={reconnecting.anchor}
          onClose={() => {
            setReconnecting(null);
          }}
        />
      ) : null}
    </>
  );
}

/** The banner under a resource card's header while something about it needs review (nothing otherwise). */
export function ClaimReviewBanner({ target, project, item }: { readonly target: PageTarget; readonly project: ProjectId; readonly item: Target }) {
  const items = useClaimHealth(target, item);
  const first = items[0];
  return first ? (
    <div className="card-review nodrag" role="status">
      <span className="card-review-text">{CLAIM_TEXT[first.kind]}</span>
      <ReviewButton target={target} project={project} items={items} />
    </div>
  ) : null;
}

/** The banner under a card's header while something about it, or a step it draws (`view`), needs review (nothing otherwise). */
export function ReviewBanner({ target, cardId, view }: { readonly target: PageTarget; readonly cardId: string; readonly view: CardView | undefined }) {
  const project = target.board.project;
  const items = useCardHealth(target, cardId, view);
  const first = items[0];
  return first ? (
    <div className="card-review nodrag" role="status">
      <span className="card-review-text">{"proposal" in first.target ? PROPOSAL_TEXT[first.kind] : BANNER_TEXT[first.kind]}</span>
      <ReviewButton target={target} project={project} items={items} />
    </div>
  ) : null;
}
