/**
 * Health on a card or a resource card: a full-width banner under the header ("Changed since it was
 * described · Review"), and the repair popover its Review opens: each problem with its repairs, every
 * repair explained before it is applied (`repairEffect`), Apply saving it at once as one undo step.
 * Reconnect first asks for the definition (or entrance) the reference became; for a proposed step, the
 * real step of its card it goes after now (an explicit placement repair, never by a similar name).
 * A card's banner also covers its groups and its proposed steps. Selected opens the same popover for a
 * connection's or a proposed step's problems (`ReviewButton`).
 */

import { useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { PATH_WORDS } from "../../board/proposeStep";
import { Button, Popover, SearchField, TextButton, showToast } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { HealthItem, HealthKind, Key, Page, ProjectId, ProposalPath, RepairKind, Target } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import {
  cardKey,
  editPage,
  findCard,
  findPage,
  findProposal,
  itemsFor,
  kindWord,
  labelTexts,
  placesAfter,
  repairBoard,
  repairEffect,
  repairLabel,
  repairSpec,
  updateProposal,
  useOpenBoard,
  type PageTarget,
} from "../../state/boards";
import { cardQuery } from "../cardData";

/** What the banner says about a card's first problem. */
const BANNER_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Changed since it was described",
  outdated: "Changed since it was described",
  missing: "Missing from the graph",
  brokenGroup: "A group no longer matches the code",
  brokenLink: "A link lost its other end",
};

/** What the banner says about a claim's (a resource card's, a connection's) first problem. */
const CLAIM_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Changed since it was drawn",
  outdated: "Changed since it was drawn",
  missing: "Cites code that is gone",
  brokenGroup: "A group no longer matches the code",
  brokenLink: "Lost an end or what it cites",
};

/** What a card's banner says when its first problem is about one of its proposed steps. */
const PROPOSAL_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Where a proposed step goes needs review",
  outdated: "Where a proposed step goes needs review",
  missing: "A proposed step lost the step it goes after",
  brokenGroup: "A group no longer matches the code",
  brokenLink: "A link lost its other end",
};

/** What a claim's review mark says about its first problem (a connection's chip, Selected). */
export function claimProblem(item: HealthItem): string {
  return CLAIM_TEXT[item.kind];
}

const NO_ITEMS: readonly HealthItem[] = [];

/** The health items about a card, the groups inside it, and its proposed steps, on its page. */
function useCardHealth(target: PageTarget, cardId: string): readonly HealthItem[] {
  const health = useOpenBoard(target.board, (board) => board.health);
  const groups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const proposals = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.proposals);
  return useMemo(() => {
    const own = new Set((groups ?? []).filter((group) => group.card === cardId).map((group) => group.id));
    const proposed = new Set((proposals ?? []).filter((proposal) => proposal.card === cardId).map((proposal) => proposal.id));
    const items = (health?.items ?? []).filter(
      (item) =>
        item.page === target.page &&
        (("card" in item.target && item.target.card === cardId) ||
          ("group" in item.target && own.has(item.target.group)) ||
          ("proposal" in item.target && proposed.has(item.target.proposal))),
    );
    return items.length > 0 ? items : NO_ITEMS;
  }, [health, groups, proposals, target.page, cardId]);
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

/** One place a proposed step can go: after a real step of its card, on one of that step's paths. */
interface StepPlace {
  readonly after: Key;
  readonly path?: ProposalPath;
  readonly label: string;
  readonly line: number | null;
}

/**
 * Reconnect for a proposed step: the real steps of its card it can go after now (each path of a
 * decision its own choice, by the placement rule `propose` follows), but the one it goes after.
 */
function StepPicker({ target, item, page, value, onChange }: { readonly target: PageTarget; readonly item: HealthItem; readonly page: Page; readonly value: StepPlace | null; readonly onChange: (place: StepPlace) => void }) {
  const proposal = "proposal" in item.target ? findProposal(page, item.target.proposal) : undefined;
  const card = proposal ? findCard(page, proposal.card) : undefined;
  const query = useQuery({ ...cardQuery(target.board.project, card ? cardKey(card) : ""), enabled: card !== undefined });
  const flow = query.data?.type === "block" && query.data.flow?.source === "current" ? query.data.flow : null;
  const labels = labelTexts(page.labels);
  const places = (flow?.nodes ?? []).flatMap((node): StepPlace[] => {
    if (node.key === undefined || node.key === proposal?.after) {
      return [];
    }
    const found = placesAfter(flow ?? { nodes: [], routes: [] }, node.key);
    if ("problem" in found) {
      return [];
    }
    const base = { after: node.key, label: labels[node.key] ?? node.label, line: node.startLine ?? null };
    return found.paths.length > 0 ? found.paths.map((path) => ({ ...base, path })) : [base];
  });
  return (
    <div className="repair-reconnect">
      <div role="listbox" aria-label="Goes after" className="repair-hits">
        {places.map((place) => (
          <button
            key={`${place.after}>${place.path ?? ""}`}
            type="button"
            role="option"
            aria-selected={value?.after === place.after && value.path === place.path}
            className="repair-hit"
            onClick={() => {
              onChange(place);
            }}
          >
            <span className="repair-hit-name">
              {place.label}
              {place.path !== undefined ? ` · ${PATH_WORDS[place.path]}` : ""}
            </span>
            <span className="repair-hit-path">{place.line !== null ? `line ${place.line}` : ""}</span>
          </button>
        ))}
        {query.isPending && card ? <p className="repair-note">Loading its steps…</p> : null}
        {!query.isPending && places.length === 0 ? <p className="repair-note">Its card has no other step a proposed step can follow right now.</p> : null}
      </div>
    </div>
  );
}

/** Reconnect for a proposed step: it goes after the step chosen (an edit of the page, one undo step), keeping its words and author. */
function ProposalReconnectRow({ target, item, page, onDone }: { readonly target: PageTarget; readonly item: HealthItem; readonly page: Page; readonly onDone: () => void }) {
  const [place, setPlace] = useState<StepPlace | null>(null);
  const proposalId = "proposal" in item.target ? item.target.proposal : null;
  const author = proposalId !== null ? findProposal(page, proposalId)?.author : undefined;
  return (
    <li className="repair-option">
      <div className="repair-option-text">
        <strong>{repairLabel("reconnect").replace(/…$/, "")}</strong>
        <span>{repairEffect("reconnect", item, page)}</span>
      </div>
      <StepPicker target={target} item={item} page={page} value={place} onChange={setPlace} />
      <Button
        variant="secondary"
        size="sm"
        disabled={place === null || proposalId === null || author === undefined}
        onClick={() => {
          if (place && proposalId !== null && author !== undefined) {
            editPage(target, (draft) => {
              updateProposal(draft, proposalId, { after: place.after, path: place.path ?? null }, author);
            });
            onDone();
          }
        }}
      >
        Apply
      </Button>
    </li>
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
  const spec = repairSpec(kind, item, page, to);
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
      {kind === "reconnect" ? <ReconnectPicker project={project} entrance={entrance} value={to} onChange={setTo} /> : null}
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
                {item.repairs.map((kind) =>
                  kind === "reconnect" && "proposal" in item.target ? (
                    <ProposalReconnectRow key={kind} target={target} item={item} page={page} onDone={onClose} />
                  ) : (
                    <RepairRow key={kind} target={target} project={project} item={item} page={page} kind={kind} onDone={onClose} />
                  ),
                )}
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
      <span className="card-review-text">{"proposal" in first.target ? PROPOSAL_TEXT[first.kind] : BANNER_TEXT[first.kind]}</span>
      <ReviewButton target={target} project={project} items={items} />
    </div>
  ) : null;
}
