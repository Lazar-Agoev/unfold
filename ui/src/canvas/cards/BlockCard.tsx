/**
 * A block card: one definition (or another indexed block) of a project in Code or Flow. The header shows
 * the project monogram (a card from another project), `ƒ`, the name, the file, and the card's own Code |
 * Flow switch (saved with the board); a review banner and the description follow. A folded card
 * (double-click its header) shows only the header, and its arrows attach there. The body never scrolls:
 * the card sizes itself to its content. Flow shows the page's labels for the card's project.
 */

import { useQuery } from "@tanstack/react-query";
import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo, type ReactNode } from "react";
import { Monogram, SegmentedControl, showToast } from "../../components";
import { CodeView } from "../../code/CodeView";
import { defaultFolds, groupConflict, groupSelection } from "../../code/outline";
import { shortPath } from "../../code/text";
import { FlowView, flowPortNodes } from "../../flow/FlowView";
import { useEngineQuery } from "../../ipc/queries";
import { layoutFlow } from "../../flow/layout";
import type { CallLink, Card, CodeText, CodeView as CodeViewModel, ProjectId, View } from "../../ipc/generated";
import {
  addGroup,
  editPage,
  findPage,
  followCall,
  keyDisplayName,
  labelsFor,
  lineOffsetIn,
  setCardView,
  setCardsFolded,
  setPendingGroup,
  updateGroup,
  updatePageView,
  useOpenBoard,
  usePageView,
  type PageTarget,
} from "../../state/boards";
import { cardQuery, isMissing, lastVerifiedCode } from "../cardData";
import { ProjectScope } from "../ProjectScope";
import { CARD_MAX_WIDTH } from "../layout";
import { useMeasure } from "../measure";
import { blockCardWidth, codeContentWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { InlineCallee } from "./InlineCallee";
import { CardGlyph, CardHandles, CardNote, DescriptionStrip, HeaderPorts, renderPorts } from "./parts";
import { ReviewBanner } from "./ReviewBanner";
import { clickCard } from "./select";

const VIEW_OPTIONS = [
  { value: "code", label: "Code" },
  { value: "flow", label: "Flow" },
] as const;

const NO_FOLDS: readonly string[] = [];
const NO_KEYS: readonly string[] = [];

export interface BlockCardProps {
  readonly target: PageTarget;
  /** The project the card resolves to. */
  readonly project: ProjectId;
  readonly card: Card & { readonly ref: { readonly block: string } };
  readonly view: View;
  readonly ports: readonly string[];
  readonly selected: boolean;
}

function toggled(list: readonly string[], key: string): string[] {
  return list.includes(key) ? list.filter((item) => item !== key) : [...list, key];
}

export function BlockCard({ target, project, card, view, ports, selected }: BlockCardProps) {
  const key = card.ref.block;
  const query = useQuery(cardQuery(project, key));
  const model = query.data?.type === "block" ? query.data : undefined;
  const measure = useMeasure();
  const updateNodeInternals = useUpdateNodeInternals();
  const folded = card.folded === true;
  const header = model?.header;
  const definition = useMemo(
    () =>
      header ? { path: header.path, kind: header.kind, name: header.name, qualifiedName: header.qualifiedName, line: header.startLine } : null,
    [header],
  );
  const source = useMemo(() => (definition ? { path: definition.path, line: definition.line, definition } : null), [definition]);
  const menu = useCardMenu(target, project, card.id, folded, true, source);

  const codeFolds = usePageView(target, (pageView) => pageView.codeFolds[card.id]);
  const expandedKeys = usePageView(target, (pageView) => pageView.expanded[card.id] ?? NO_KEYS);
  const openChains = usePageView(target, (pageView) => pageView.flowChains[card.id] ?? NO_KEYS);
  const selectedKey = usePageView(target, ({ selection }) => (selection.kind === "card" && selection.cardId === card.id ? selection.key : null));
  // A selected line from or to this card: the caller's line keeps the selection fill, the callee
  // marks the lines the contract reads (the contract's query, shared with the contract itself).
  const line = usePageView(target, ({ selection }) => (selection.kind === "line" && (selection.from === card.id || selection.to === card.id) ? selection : null));
  const calleeOf = line !== null && line.to === card.id ? line : null;
  const contract = useEngineQuery(
    "call_contract",
    { project: calleeOf?.project ?? project, source: calleeOf?.source ?? "", seq: calleeOf?.seq ?? 0 },
    { enabled: calleeOf !== null },
  );
  const calleeLines = calleeOf !== null ? contract.data?.calleeLines : undefined;
  const marked = useMemo(() => (calleeLines ? new Set(calleeLines) : undefined), [calleeLines]);
  const callerLine = line !== null && line.from === card.id ? line.line : null;
  const pending = usePageView(target, ({ pendingGroup }) => (pendingGroup?.cardId === card.id ? pendingGroup : null));
  const allGroups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const labels = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.labels);
  const health = useOpenBoard(target.board, (board) => board.health);
  const groups = useMemo(() => {
    const broken = new Set(
      (health?.items ?? []).flatMap((item) => (item.page === target.page && item.kind === "brokenGroup" && "group" in item.target ? [item.target.group] : [])),
    );
    return (allGroups ?? []).filter((group) => group.card === card.id && !broken.has(group.id));
  }, [allGroups, health, card.id, target.page]);

  // Code: the verified source, or the last verified render kept in memory while the file changed.
  const codeModel: CodeViewModel | null = model?.code ?? null;
  const stale = codeModel?.source.state === "changed" ? lastVerifiedCode(project, key) : undefined;
  const shown: CodeText | null =
    codeModel?.source.state === "current" ? codeModel.source : stale?.source.state === "current" ? stale.source : null;
  const folds = useMemo(() => new Set(codeFolds ?? (shown ? defaultFolds(shown.outline) : NO_FOLDS)), [codeFolds, shown]);
  const portSet = useMemo(() => new Set(ports), [ports]);
  // The page's labels for this card's project (`ref.project` names it the same way labels do).
  const labelText = useMemo(() => labelsFor(labels ?? [], card.ref.project), [labels, card.ref.project]);
  const chainSet = useMemo(() => new Set(openChains), [openChains]);

  // Laid out only while the card shows Flow: measuring and placing nodes is the costly part of a card.
  const flowModel = view === "flow" ? (model?.flow ?? null) : null;
  const flowLayout = useMemo(
    () => (flowModel ? layoutFlow(flowModel, { labels: labelText, openChains: chainSet, measure, maxWidth: CARD_MAX_WIDTH - 2 }) : null),
    [flowModel, labelText, chainSet, measure],
  );

  const name = model?.header.qualifiedName ?? keyDisplayName(key);
  const location = model ? shortPath(model.header.path) : "";
  // A card from another project names it with its monogram (`ref.project` is its shared name).
  const foreign = card.ref.project;
  const bodyWidth = folded ? null : view === "flow" && flowLayout ? flowLayout.width + 2 : shown ? codeContentWidth(shown, measure) : null;
  // `measure` changes when the bundled fonts load, so widths come from the real fonts.
  const width = useMemo(
    () => blockCardWidth({ name, location, monogram: foreign !== undefined, body: bodyWidth }, measure),
    [name, location, foreign, bodyWidth, measure],
  );

  // Which ports the body places; the rest (and all of a folded card's) sit on the header.
  const flowPorts = useMemo(() => {
    if (view !== "flow" || !flowLayout) {
      return null;
    }
    const owners = new Map<string, string>();
    for (const id of ports) {
      owners.set(id, shown?.links.find((link) => link.source === id)?.owner ?? "");
    }
    return flowPortNodes(flowLayout, owners);
  }, [view, flowLayout, ports, shown]);
  const headerPorts = folded
    ? ports
    : view === "flow" && flowPorts
      ? flowPorts.unplaced
      : shown
        ? ports.filter((id) => !shown.links.some((link) => link.source === id))
        : ports;

  const portsKey = ports.join("\u0000");
  const foldsKey = [...folds].join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, foldsKey, folded, view, width, model, expandedKeys, openChains, groups, pending]);

  const selectKey = (blockKey: string) => {
    clickCard(target, card.id, blockKey);
  };
  const follow = (targetKey: string, anchor: HTMLElement, line?: number) => {
    const offset = lineOffsetIn(anchor);
    followCall({
      target,
      fromCardId: card.id,
      fromLabel: name,
      targetKey,
      targetLabel: keyDisplayName(targetKey),
      ...(offset !== undefined ? { lineOffset: offset } : {}),
      ...(line !== undefined ? { line } : {}),
    });
  };
  const onFollowLink = (link: CallLink, anchor: HTMLElement) => {
    if (link.target !== undefined) {
      follow(link.target, anchor, link.line);
    }
  };
  const onExpand = (link: CallLink) => {
    updatePageView(target, (draft) => {
      draft.expanded[card.id] = toggled(draft.expanded[card.id] ?? [], link.source);
    });
  };
  const expanded = useMemo(() => {
    const map = new Map<string, ReactNode>();
    for (const occurrence of expandedKeys) {
      const callee = shown?.links.find((link) => link.source === occurrence)?.target;
      if (callee !== undefined) {
        map.set(
          occurrence,
          <InlineCallee
            project={project}
            targetKey={callee}
            onClose={() => {
              updatePageView(target, (draft) => {
                draft.expanded[card.id] = (draft.expanded[card.id] ?? []).filter((item) => item !== occurrence);
              });
            }}
          />,
        );
      }
    }
    return map;
  }, [expandedKeys, shown, target, project, card.id]);

  const onGutterSelect = (from: number, to: number) => {
    if (!shown) {
      return;
    }
    const selection = groupSelection(shown.outline, from, to);
    if ("error" in selection) {
      setPendingGroup(target, null);
      showToast({ title: selection.error });
      return;
    }
    const conflict = groupConflict(shown.outline, selection.members, groups);
    setPendingGroup(target, { cardId: card.id, members: selection.members, ...(conflict !== null ? { error: conflict } : {}) });
  };
  const onNamePendingGroup = (title: string | null) => {
    if (title === null || title.trim().length === 0 || !pending || pending.error !== undefined) {
      setPendingGroup(target, null);
      return;
    }
    editPage(target, (page) => addGroup(page, card.id, pending.members, title.trim()));
    setPendingGroup(target, null);
  };

  const setView = (next: View) => {
    editPage(target, (page) => {
      setCardView(page, card.id, next);
    });
    updatePageView(target, (draft) => {
      if (draft.lens !== null && !draft.lensExceptions.includes(card.id)) {
        draft.lensExceptions.push(card.id);
      }
    });
  };
  const toggleFolded = () => {
    editPage(target, (page) => {
      setCardsFolded(page, [card.id], !folded);
    });
  };

  let body: ReactNode;
  if (query.isPending) {
    body = <CardNote>Loading…</CardNote>;
  } else if (query.isError) {
    body = <CardNote warn>{isMissing(query.error) ? "This reference is missing from the graph." : query.error.message}</CardNote>;
  } else if (view === "flow" && model?.flow && flowLayout && flowPorts) {
    body = (
      <FlowView
        model={model.flow}
        layout={flowLayout}
        width={width - 2}
        selectedKey={selectedKey ?? null}
        onSelect={selectKey}
        onFollow={(_node, target, anchor) => {
          follow(target, anchor);
        }}
        onOpenCallback={(callback, anchor) => {
          follow(callback.key, anchor);
        }}
        onToggleChain={(chainKey) => {
          updatePageView(target, (draft) => {
            draft.flowChains[card.id] = toggled(draft.flowChains[card.id] ?? [], chainKey);
          });
        }}
        ports={flowPorts.byNode}
        renderPorts={renderPorts}
      />
    );
  } else if (shown && model) {
    body = (
      <>
        {stale ? (
          <CardNote warn>
            {model.header.fileState === "last_good" ? "Doesn't parse right now · showing the last verified code" : "Updating · showing the last verified code"}
          </CardNote>
        ) : null}
        <CodeView
          code={shown}
          language={model.header.language}
          label={name}
          folds={folds}
          onToggleFold={(foldKey) => {
            updatePageView(target, (draft) => {
              draft.codeFolds[card.id] = toggled(draft.codeFolds[card.id] ?? [...folds], foldKey);
            });
          }}
          groups={groups}
          onToggleGroup={(groupId) => {
            const group = groups.find((item) => item.id === groupId);
            editPage(target, (page) => {
              updateGroup(page, groupId, { collapsed: !(group?.collapsed ?? false) });
            });
          }}
          onRenameGroup={(groupId, title) => {
            editPage(target, (page) => {
              updateGroup(page, groupId, { title });
            });
          }}
          onGroupMenu={menu.openGroupMenu}
          ports={portSet}
          renderPorts={renderPorts}
          selectedKey={selectedKey ?? null}
          onSelect={selectKey}
          onFollow={onFollowLink}
          onExpand={onExpand}
          expanded={expanded}
          onGutterSelect={onGutterSelect}
          pendingGroup={pending}
          onNamePendingGroup={onNamePendingGroup}
          dimmed={stale !== undefined}
          {...(marked ? { marked } : {})}
          callerLine={callerLine}
        />
      </>
    );
  } else if (codeModel?.source.state === "changed") {
    body = <CardNote warn>{model?.header.fileState === "last_good" ? "This file doesn't parse right now." : "The source changed · updating."}</CardNote>;
  } else {
    body = <CardNote warn>Source unavailable.</CardNote>;
  }

  const missing = query.isError && isMissing(query.error);
  return (
    <article
      className={`board-card block-card${selected ? " selected" : ""}${folded ? " folded" : ""}${missing ? " missing" : ""}`}
      data-card-id={card.id}
      data-key={key}
      data-loading={query.isPending ? "true" : undefined}
      style={{ width }}
      aria-label={name}
      onClick={() => {
        selectKey(key);
      }}
      onContextMenu={menu.onContextMenu}
    >
      <CardHandles />
      <header className="card-header card-drag" onDoubleClick={toggleFolded}>
        {foreign !== undefined ? <Monogram name={foreign} size={20} /> : null}
        <CardGlyph kind={model?.header.kind ?? null} />
        <h3 className="card-title" title={model?.header.qualifiedName ?? key}>
          {name}
        </h3>
        <span className="card-location" title={model ? `${model.header.path}:${model.header.startLine}` : undefined}>
          {location}
        </span>
        {!folded && model ? (
          <div
            className="nodrag"
            onClick={(event) => {
              event.stopPropagation();
            }}
          >
            <SegmentedControl options={VIEW_OPTIONS} value={view} onChange={setView} label={`View of ${name}`} variant="mini" />
          </div>
        ) : null}
        <HeaderPorts ids={headerPorts} />
      </header>
      <ReviewBanner target={target} project={project} cardId={card.id} />
      {folded ? null : (
        <>
          <DescriptionStrip target={target} cardId={card.id} description={card.description} />
          <div className="card-body nodrag nopan">
            <ProjectScope.Provider value={project}>{body}</ProjectScope.Provider>
          </div>
        </>
      )}
      {menu.element}
    </article>
  );
}
