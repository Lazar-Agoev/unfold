/**
 * A block card: one definition (or another indexed block) of a project in Code (the Code lens) or Flow
 * (expanded in the Diagram lens, where its header starts with the chevron that makes it compact again).
 * The header shows `ƒ`, the name, and the file; a review banner and the description follow. The body
 * never scrolls: the card sizes itself to its content.
 * Flow shows the page's labels and the card's groups as step groups (a group's
 * one `collapsed` field folds it in both views), and the calls the card expands in place
 * (`CallInsets`): an inset under the call, while the call's own arrow keeps leaving from its step and
 * the calls the inset shows leave from theirs (`placeFlowPorts`).
 * In Flow, Shift-click selects a run of its own steps (Combine), `</>` on a step or a combined block
 * opens its source (View code), and right-click opens a step's menu. Its proposed steps show in its
 * own Flow only (never in Code, nor in another card's inset), with the missing or held treatment
 * their health gives them.
 */

import { useQuery } from "@tanstack/react-query";
import { useUpdateNodeInternals } from "@xyflow/react";
import { useLayoutEffect, useMemo, type ReactNode } from "react";
import { stepRun } from "../../board/combine";
import { viewCode } from "../../board/viewCode";
import { CodeView, portSpot } from "../../code/CodeView";
import { defaultFolds } from "../../code/outline";
import { shortPath } from "../../code/text";
import { FlowView, type FlowProposals, type ProposalMark } from "../../flow/FlowView";
import { stepGroups } from "../../flow/groups";
import { useEngineQuery } from "../../ipc/queries";
import { insetRoom } from "../../flow/insets";
import { layoutFlow, type ProposalSpec } from "../../flow/layout";
import type { CallLink, Card, CodeText, CodeView as CodeViewModel, HealthItem, Key, ProjectId, Proposal, View } from "../../ipc/generated";
import {
  editGroup,
  editPage,
  findPage,
  followCall,
  getPageView,
  keyDisplayName,
  labelTexts,
  lineOffsetIn,
  select,
  updateGroup,
  updatePageView,
  useOpenBoard,
  usePageView,
  type PageTarget,
  type Selection,
} from "../../state/boards";
import { cardQuery, isMissing, lastVerifiedCode } from "../cardData";
import { CARD_MAX_WIDTH } from "../layout";
import { useMeasure } from "../measure";
import { blockCardWidth, codeContentWidth } from "../sizing";
import { useCardMenu } from "./cardMenu";
import { flowExpansion, placeFlowPorts, useCallInsets, type InsetContext } from "./CallInsets";
import { InlineCallee } from "./InlineCallee";
import { CardGlyph, CardHandles, CardNote, DescriptionStrip, ExpandToggle, HeaderPorts, renderPorts } from "./parts";
import { ReviewBanner } from "./ReviewBanner";
import { clickCard } from "./select";

const NO_FOLDS: readonly string[] = [];
const NO_KEYS: readonly string[] = [];
const NO_PROPOSALS: readonly Proposal[] = [];
const NO_INSET_PORTS: ReadonlyMap<string, ReadonlyMap<string, readonly string[]>> = new Map();

/** Why where a proposed step goes needs review: its step is missing, or held. */
const PROPOSAL_NOTES = {
  missing: "The step it went after is missing.",
  held: "The step it goes after is held for review.",
} as const;

/** The health of each proposed step of a page, by id: missing or held. */
function proposalStates(items: readonly HealthItem[] | undefined, page: string): Map<string, "missing" | "held"> {
  const states = new Map<string, "missing" | "held">();
  for (const item of items ?? []) {
    if (item.page === page && "proposal" in item.target && (item.kind === "missing" || item.kind === "review")) {
      states.set(item.target.proposal, item.kind === "missing" ? "missing" : "held");
    }
  }
  return states;
}
/** The widest a card's own Flow may be: the card's widest less its border. */
const FLOW_MAX_WIDTH = CARD_MAX_WIDTH - 2;

export interface BlockCardProps {
  readonly target: PageTarget;
  /** The project the card resolves to. */
  readonly project: ProjectId;
  readonly card: Card & { readonly ref: { readonly block: string } };
  /** Code or Flow: a Diagram node and a type card's fields are their own cards. */
  readonly view: Exclude<View, "diagram">;
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
  const header = model?.header;
  const menu = useCardMenu(target, card.id, true, header?.qualifiedName ?? keyDisplayName(key));

  const codeFolds = usePageView(target, (pageView) => pageView.codeFolds[card.id]);
  const expandedKeys = usePageView(target, (pageView) => pageView.expanded[card.id] ?? NO_KEYS);
  const openChains = usePageView(target, (pageView) => pageView.flowChains[card.id] ?? NO_KEYS);
  // A step of the card's own Flow, or (with its owner) one shown in a call expanded in place.
  const selectedKey = usePageView(target, ({ selection }) =>
    selection.kind === "card" && selection.cardId === card.id && selection.owner === undefined ? selection.key : null,
  );
  const insetKey = usePageView(target, ({ selection }) =>
    selection.kind === "card" && selection.cardId === card.id && selection.owner !== undefined ? selection.key : null,
  );
  const insetOwner = usePageView(target, ({ selection }) => (selection.kind === "card" && selection.cardId === card.id ? selection.owner : undefined));
  // The last step of a Shift-selected run of the card's own steps, which starts at `selectedKey`.
  const runLast = usePageView(target, ({ selection }) =>
    selection.kind === "card" && selection.cardId === card.id && selection.owner === undefined ? (selection.last ?? null) : null,
  );
  // A selected line from or to this card: the caller's line keeps the selection fill, the callee
  // marks the lines the contract reads (the contract's query, shared with the contract itself).
  const line = usePageView(target, ({ selection }) => (selection.kind === "line" && (selection.from === card.id || selection.to === card.id) ? selection : null));
  const calleeOf = line !== null && line.to === card.id ? line : null;
  const contract = useEngineQuery(
    "call_contract",
    { project, source: calleeOf?.source ?? "", seq: calleeOf?.seq ?? 0 },
    { enabled: calleeOf !== null },
  );
  const calleeLines = calleeOf !== null ? contract.data?.calleeLines : undefined;
  const marked = useMemo(() => (calleeLines ? new Set(calleeLines) : undefined), [calleeLines]);
  const callerLine = line !== null && line.from === card.id ? line.line : null;
  const allGroups = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.groups);
  const labels = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.labels);
  const health = useOpenBoard(target.board, (board) => board.health);
  const broken = useMemo(
    () =>
      new Set(
        (health?.items ?? []).flatMap((item) => (item.page === target.page && item.kind === "brokenGroup" && "group" in item.target ? [item.target.group] : [])),
      ),
    [health, target.page],
  );
  const cardGroups = useMemo(() => (allGroups ?? []).filter((group) => group.card === card.id), [allGroups, card.id]);
  const allProposals = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.proposals) ?? NO_PROPOSALS;
  const selectedProposal = usePageView(target, ({ selection }) => (selection.kind === "proposal" ? selection.proposalId : null));
  // The card's proposed steps: what the layout keeps room for, and how each is marked.
  const proposed = useMemo(() => {
    const states = proposalStates(health?.items, target.page);
    const own = allProposals.filter((proposal) => proposal.card === card.id);
    const specs: ProposalSpec[] = own.map((proposal) => {
      const state = states.get(proposal.id);
      return {
        id: proposal.id,
        after: proposal.after,
        ...(proposal.path !== undefined ? { path: proposal.path } : {}),
        title: proposal.title,
        ...(state !== undefined ? { note: PROPOSAL_NOTES[state] } : {}),
      };
    });
    const marks = new Map<string, ProposalMark>(
      own.map((proposal) => {
        const state = states.get(proposal.id);
        return [proposal.id, { author: proposal.author, ...(state !== undefined ? { state } : {}) }];
      }),
    );
    return { specs, marks };
  }, [allProposals, card.id, health, target.page]);
  const groups = useMemo(() => cardGroups.filter((group) => !broken.has(group.id)), [cardGroups, broken]);
  // A run's steps, each drawn selected (its statements' steps when it can be combined).
  const runKeys = useMemo(() => {
    const run = runLast !== null && selectedKey !== null && model ? stepRun(model, selectedKey, runLast, cardGroups) : null;
    return run ? new Set(run.steps.flatMap((step) => (step.key !== undefined ? [step.key] : []))) : undefined;
  }, [runLast, selectedKey, model, cardGroups]);

  // Code: the verified source, or the last verified render kept in memory while the file changed.
  const codeModel: CodeViewModel | null = model?.code ?? null;
  const stale = codeModel?.source.state === "changed" ? lastVerifiedCode(project, key) : undefined;
  const shown: CodeText | null =
    codeModel?.source.state === "current" ? codeModel.source : stale?.source.state === "current" ? stale.source : null;
  const folds = useMemo(() => new Set(codeFolds ?? (shown ? defaultFolds(shown.outline) : NO_FOLDS)), [codeFolds, shown]);
  const portSet = useMemo(() => new Set(ports), [ports]);
  // The page's labels, by key.
  const labelText = useMemo(() => labelTexts(labels ?? []), [labels]);
  const chainSet = useMemo(() => new Set(openChains), [openChains]);

  // Laid out only while the card shows Flow: measuring and placing nodes is the costly part of a card.
  const flowModel = view === "flow" ? (model?.flow ?? null) : null;
  const flowGroups = useMemo(() => (flowModel && shown ? stepGroups(groups, shown.outline) : []), [flowModel, groups, shown]);
  const insets = useCallInsets({
    target,
    project,
    card,
    flow: flowModel?.source === "current" ? flowModel : null,
    labels: labelText,
    openChains: chainSet,
    groups: allGroups,
    broken,
    measure,
    maxWidth: FLOW_MAX_WIDTH,
  });
  const flowLayout = useMemo(
    () =>
      flowModel
        ? layoutFlow(flowModel, {
            labels: labelText,
            openChains: chainSet,
            groups: flowGroups,
            measure,
            maxWidth: FLOW_MAX_WIDTH,
            insets: insetRoom(insets),
            proposals: proposed.specs,
          })
        : null,
    [flowModel, labelText, chainSet, flowGroups, measure, insets, proposed.specs],
  );
  const flowProposals: FlowProposals = {
    marks: proposed.marks,
    selected: selectedProposal,
    onSelect: (proposalId) => {
      if (getPageView(target).linking) {
        clickCard(target, card.id, null);
      } else {
        select(target, { kind: "proposal", proposalId });
      }
    },
  };
  const selectedGroup = usePageView(target, ({ selection }) => (selection.kind === "group" ? selection.groupId : null));

  const name = model?.header.qualifiedName ?? keyDisplayName(key);
  const location = model ? shortPath(model.header.path) : "";
  const bodyWidth = view === "flow" && flowLayout ? flowLayout.width + 2 : shown ? codeContentWidth(shown, measure) : null;
  // A call expanded in place may need more than a card's usual widest; the card grows rather than clip it.
  const insetWidth = flowLayout && flowLayout.insets.length > 0 ? flowLayout.width + 2 : 0;
  // `measure` changes when the bundled fonts load, so widths come from the real fonts.
  const width = useMemo(
    () => Math.max(insetWidth, blockCardWidth({ name, location, chevron: view === "flow", body: bodyWidth }, measure)),
    [name, location, view, bodyWidth, insetWidth, measure],
  );

  // Which ports the body places (its own Flow's steps, or an inset's); the rest sit on the header.
  const flowPorts = useMemo(() => {
    if (view !== "flow" || !flowLayout) {
      return null;
    }
    return placeFlowPorts(key, flowLayout, insets, ports, (id) => shown?.links.find((link) => link.source === id)?.owner ?? "");
  }, [view, flowLayout, insets, ports, shown, key]);
  const headerPorts =
    view === "flow" && flowPorts ? flowPorts.unplaced : shown ? ports.filter((id) => portSpot(shown, id) === undefined) : ports;

  const portsKey = ports.join("\u0000");
  const foldsKey = [...folds].join("\u0000");
  useLayoutEffect(() => {
    updateNodeInternals(card.id);
  }, [updateNodeInternals, card.id, portsKey, foldsKey, view, width, model, expandedKeys, openChains, groups, flowLayout]);

  const selectKey = (blockKey: string) => {
    clickCard(target, card.id, blockKey);
  };
  // Shift-click: a run from the selected step of the card's own Flow to this one (else a plain click).
  const extend = (blockKey: Key) => {
    const { selection, linking } = getPageView(target);
    const first = selection.kind === "card" && selection.cardId === card.id && selection.owner === undefined ? selection.key : null;
    if (linking || first === null || first === key || first === blockKey) {
      selectKey(blockKey);
      return;
    }
    select(target, { kind: "card", cardId: card.id, key: first, last: blockKey });
  };
  // `</>`: selects what it is on and opens its source; the sheet then follows the selection.
  const showCode = (selection: Selection) => {
    select(target, selection);
    viewCode(target, selection);
  };
  // A step group in Flow selects the group, unless a link is being drawn to this card.
  const selectGroup = (groupId: string) => {
    if (getPageView(target).linking) {
      clickCard(target, card.id, null);
    } else {
      select(target, { kind: "group", groupId });
    }
  };
  const follow = (targetKey: string, anchor: HTMLElement) => {
    const offset = lineOffsetIn(anchor);
    followCall({
      target,
      fromCardId: card.id,
      targetKey,
      ...(offset !== undefined ? { lineOffset: offset } : {}),
    });
  };
  const onFollowLink = (link: CallLink, anchor: HTMLElement) => {
    if (link.target !== undefined) {
      follow(link.target, anchor);
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

  const toggleChain = (chainKey: string) => {
    updatePageView(target, (draft) => {
      draft.flowChains[card.id] = toggled(draft.flowChains[card.id] ?? [], chainKey);
    });
  };
  const toggleGroup = (groupId: string) => {
    const group = (allGroups ?? []).find((item) => item.id === groupId);
    if (group) {
      editGroup(target, groupId, { collapsed: !group.collapsed });
    }
  };
  const insetContext: InsetContext = {
    target,
    project,
    cardId: card.id,
    paths: card.expanded ?? [],
    selected: insetKey !== null && insetOwner !== undefined ? { key: insetKey, owner: insetOwner } : null,
    selectedGroup,
    follow: (targetKey, anchor) => {
      follow(targetKey, anchor);
    },
    onToggleChain: toggleChain,
    onSelectGroup: selectGroup,
    onToggleGroup: toggleGroup,
    onViewStep: (stepKey, owner) => {
      showCode({ kind: "card", cardId: card.id, key: stepKey, owner });
    },
    onViewGroup: (groupId) => {
      showCode({ kind: "group", groupId });
    },
    onStepMenu: menu.openStepMenu,
    ports: flowPorts?.insets ?? NO_INSET_PORTS,
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
        onToggleChain={toggleChain}
        selectedGroup={selectedGroup}
        onSelectGroup={selectGroup}
        onToggleGroup={toggleGroup}
        onGroupMenu={menu.openGroupMenu}
        onExtend={extend}
        {...(runKeys ? { runKeys } : {})}
        onViewCode={(stepKey) => {
          showCode({ kind: "card", cardId: card.id, key: stepKey });
        }}
        onViewGroupCode={(groupId) => {
          showCode({ kind: "group", groupId });
        }}
        onStepMenu={(_key, at) => {
          menu.openStepMenu(at);
        }}
        ports={flowPorts.byNode}
        renderPorts={renderPorts}
        expansion={flowExpansion(insetContext, [], [key], insets, { width: Math.max(flowLayout.width, width - 2), reach: 0 })}
        proposals={flowProposals}
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
          project={project}
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
      className={`board-card block-card${selected ? " selected" : ""}${missing ? " missing" : ""}`}
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
      <header className="card-header card-drag">
        {view === "flow" ? <ExpandToggle target={target} cardId={card.id} expanded name={name} /> : null}
        <CardGlyph kind={model?.header.kind ?? null} />
        <h3 className="card-title" title={model?.header.qualifiedName ?? key}>
          {name}
        </h3>
        <span className="card-location" title={model ? `${model.header.path}:${model.header.startLine}` : undefined}>
          {location}
        </span>
        <HeaderPorts ids={headerPorts} />
      </header>
      <ReviewBanner target={target} project={project} cardId={card.id} />
      <DescriptionStrip target={target} cardId={card.id} description={card.description} />
      <div className="card-body nodrag nopan">{body}</div>
      {menu.element}
    </article>
  );
}
