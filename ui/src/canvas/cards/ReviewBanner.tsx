/**
 * Health on a card or a resource card: a full-width banner under the header ("Changed since it was
 * described · Review"), and the repair popover its Review opens: each problem with its repairs, every
 * repair explained before it is applied (`repairEffect`), Apply saving it at once as one undo step.
 * Reconnect first asks for the definition (or entrance) the reference became. Selected opens the same
 * popover for a connection's problems (`ReviewButton`).
 */

import { useMemo, useState } from "react";
import { Button, Popover, SearchField, TextButton, showToast } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { HealthItem, HealthKind, Page, ProjectId, RepairKind, Target } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import { cardKey, findCard, findPage, itemsFor, kindWord, repairBoard, repairEffect, repairLabel, repairSpec, useOpenBoard, type PageTarget } from "../../state/boards";
import { resolveRefProject, useProjectIndex } from "../../state/workspace";

/** What the banner says about a card's first problem. */
const BANNER_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Changed since it was described",
  outdated: "Changed since it was described",
  missing: "Missing from the graph",
  brokenGroup: "A group no longer matches the code",
  notConnected: "From a project that isn't connected",
  brokenLink: "A link lost its other end",
};

/** What the banner says about a claim's (a resource card's, a connection's) first problem. */
const CLAIM_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Changed since it was drawn",
  outdated: "Changed since it was drawn",
  missing: "Cites code that is gone",
  brokenGroup: "A group no longer matches the code",
  notConnected: "Cites a project that isn't connected",
  brokenLink: "Lost an end or what it cites",
};

/** What a claim's review mark says about its first problem (a connection's chip, Selected). */
export function claimProblem(item: HealthItem): string {
  return CLAIM_TEXT[item.kind];
}

const NO_ITEMS: readonly HealthItem[] = [];

/** The health items about a card and the groups inside it, on its page. */
function useCardHealth(target: PageTarget, cardId: string): readonly HealthItem[] {
  const health = useOpenBoard(target.board, (board) => board.health);
  const groups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  return useMemo(() => {
    const own = new Set((groups ?? []).filter((group) => group.card === cardId).map((group) => group.id));
    const items = (health?.items ?? []).filter(
      (item) => item.page === target.page && (("card" in item.target && item.target.card === cardId) || ("group" in item.target && own.has(item.target.group))),
    );
    return items.length > 0 ? items : NO_ITEMS;
  }, [health, groups, target.page, cardId]);
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

function RepairRow({
  target,
  project,
  item,
  page,
  kind,
  onDone,
}: {
  readonly target: PageTarget;
  readonly project: ProjectId;
  readonly item: HealthItem;
  readonly page: Page;
  readonly kind: RepairKind;
  readonly onDone: () => void;
}) {
  const [to, setTo] = useState("");
  const [busy, setBusy] = useState(false);
  const index = useProjectIndex();
  const spec = repairSpec(kind, item, page, to);
  // What a claim cites may be in another project: Reconnect searches there.
  const searchProject = item.project !== undefined ? (resolveRefProject(index, target.board.project, item.project) ?? project) : project;
  const apply = async () => {
    if (!spec) {
      return;
    }
    setBusy(true);
    try {
      await repairBoard(target.board, spec);
      onDone();
    } catch (error) {
      showToast({ title: "Couldn't repair the board", body: toUnfoldError(error).message, tone: "danger" });
      setBusy(false);
    }
  };
  const about = "card" in item.target ? findCard(page, item.target.card) : undefined;
  const entrance = (item.key ?? (about ? cardKey(about) : "")).startsWith("entrance:");
  return (
    <li className="repair-option">
      <div className="repair-option-text">
        <strong>{repairLabel(kind).replace(/…$/, "")}</strong>
        <span>{repairEffect(kind, item, page)}</span>
      </div>
      {kind === "reconnect" ? <ReconnectPicker project={searchProject} entrance={entrance} value={to} onChange={setTo} /> : null}
      <Button variant="secondary" size="sm" busy={busy} disabled={!spec} onClick={() => void apply()}>
        Apply
      </Button>
    </li>
  );
}

/** The repair popover of a card's problems, anchored to its Review button. */
function RepairPopover({
  target,
  project,
  items,
  anchor,
  onClose,
}: {
  readonly target: PageTarget;
  readonly project: ProjectId;
  readonly items: readonly HealthItem[];
  readonly anchor: HTMLElement | null;
  readonly onClose: () => void;
}) {
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  return (
    <Popover open anchor={anchor} label="Review" onClose={onClose} side="bottom" align="end" width={340} className="repair-popover">
      {page
        ? items.map((item, index) => (
            <section key={`${item.kind}:${index}`} className="repair-item">
              <h3 className="repair-title">{kindWord(item.kind)}</h3>
              <p className="repair-message">{item.message}</p>
              <ul className="repair-options">
                {item.repairs.map((kind) => (
                  <RepairRow key={kind} target={target} project={project} item={item} page={page} kind={kind} onDone={onClose} />
                ))}
              </ul>
            </section>
          ))
        : null}
      <p className="repair-note">A repair is saved right away; ⌘Z undoes it.</p>
    </Popover>
  );
}

/** "Review", opening the repair popover of `items` (nothing when there are none). */
export function ReviewButton({ target, project, items }: { readonly target: PageTarget; readonly project: ProjectId; readonly items: readonly HealthItem[] }) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  if (items.length === 0) {
    return null;
  }
  return (
    <>
      <TextButton
        className="card-review-action"
        aria-expanded={anchor !== null}
        onClick={(event) => {
          event.stopPropagation();
          const button = event.currentTarget;
          setAnchor((current) => (current ? null : button));
        }}
      >
        Review
      </TextButton>
      {anchor ? (
        <RepairPopover
          target={target}
          project={project}
          items={items}
          anchor={anchor}
          onClose={() => {
            setAnchor(null);
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

/** The banner under a card's header while something about it needs review (nothing otherwise). */
export function ReviewBanner({ target, project, cardId }: { readonly target: PageTarget; readonly project: ProjectId; readonly cardId: string }) {
  const items = useCardHealth(target, cardId);
  const first = items[0];
  return first ? (
    <div className="card-review nodrag" role="status">
      <span className="card-review-text">{BANNER_TEXT[first.kind]}</span>
      <ReviewButton target={target} project={project} items={items} />
    </div>
  ) : null;
}
