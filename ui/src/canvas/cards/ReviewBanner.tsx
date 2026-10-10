/**
 * Health on a card (redesign §4.10, 02 §4.4): a full-width banner under the header ("Changed since it
 * was described · Review"), and the repair popover its Review opens: each problem with its repairs,
 * every repair explained before it is applied (`repairEffect`), Apply saving it at once as one undo
 * step. Reconnect first asks for the definition (or entrance) the reference became.
 */

import { useMemo, useState } from "react";
import { Button, Popover, SearchField, TextButton, showToast } from "../../components";
import { useEngineQuery } from "../../ipc";
import type { HealthItem, HealthKind, Page, ProjectId, RepairKind } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import { cardKey, findCard, findPage, kindWord, repairBoard, repairEffect, repairLabel, repairSpec, useOpenBoard, type PageTarget } from "../../state/boards";

/** What the banner says about a card's first problem. */
const BANNER_TEXT: Readonly<Record<HealthKind, string>> = {
  review: "Changed since it was described",
  outdated: "Changed since it was described",
  missing: "Missing from the graph",
  brokenGroup: "A group no longer matches the code",
  notConnected: "From a project that isn't connected",
  brokenLink: "A link lost its other end",
};

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

/** The banner under a card's header while something about it needs review (nothing otherwise). */
export function ReviewBanner({ target, project, cardId }: { readonly target: PageTarget; readonly project: ProjectId; readonly cardId: string }) {
  const items = useCardHealth(target, cardId);
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const first = items[0];
  if (!first) {
    return null;
  }
  return (
    <div className="card-review nodrag" role="status">
      <span className="card-review-text">{BANNER_TEXT[first.kind]}</span>
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
    </div>
  );
}
