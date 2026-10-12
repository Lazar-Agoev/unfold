/**
 * Context menus on the canvas: a card's (Open source, Show callers, Draw link…, Add as entrance…, Copy
 * link, Fold or Unfold card, Remove from page), a Flow step's (View code, Combine…, Add proposed step
 * after…), and a group's (View code, Collapse, Describe, Copy link, Ungroup). Open source and View code
 * open the peek sheet through the one View code resolver, highlighted; Combine… is enabled only for a
 * run of steps that can be combined, and says why otherwise; Add proposed step after… shows only on a
 * step a proposed step can follow, as a submenu of its actual paths for a decision. Each page choice is
 * one board edit, so ⌘Z undoes it.
 */

import type { MouseEvent, ReactNode } from "react";
import { copyLink } from "../../agent";
import { PATH_WORDS, PROPOSAL_TITLE_FIELD, proposalChoiceOf, proposeStep, type ProposalChoice } from "../../board/proposeStep";
import { combineOf, sourceOf, viewCode } from "../../board/viewCode";
import { useContextMenu, type MenuEntry } from "../../components";
import type { ProjectId, ProposalPath } from "../../ipc/generated";
import { ActionIds, actions, requestFocus } from "../../keyboard";
import { canBeHandler } from "../../launches";
import { useAuthor } from "../../state/author";
import { editGroup, editPage, findPage, getOpenBoard, getPageView, removeCard, select, setCardsFolded, setLinking, ungroup, type PageTarget, type Selection } from "../../state/boards";
import { setSelectedCollapsed } from "../../state/layout";
import { openDialog, type DefinitionRef } from "../../state/overlays";
import { showCallers } from "../showCallers";

function stop(event: MouseEvent): void {
  event.stopPropagation();
}

/** Expands the sidebar's Selected section on the page's selection and focuses one of its fields. */
export function showInSelected(field: string): void {
  setSelectedCollapsed(false);
  requestFocus(field);
}

/** Where a card's source is: a block's definition, or an entrance's declaration. */
interface CardSource {
  readonly path: string;
  readonly line: number | null;
  /** A block's definition, which may become an entrance's handler. */
  readonly definition?: DefinitionRef;
}

interface CardMenu {
  readonly onContextMenu: (event: MouseEvent) => void;
  readonly openGroupMenu: (groupId: string, at: MouseEvent | Element) => void;
  /** Opens the menu of the selection a Flow step's right-click left: the step, or the run it is in. */
  readonly openStepMenu: (at: MouseEvent | Element) => void;
  readonly element: ReactNode;
}

/** View code for `selection`: enabled when it has a source, else disabled with why. */
function viewCodeEntry(target: PageTarget, selection: Selection): MenuEntry {
  const source = sourceOf(target, selection);
  const shortcut = actions.shortcutsFor(ActionIds.viewCode)[0];
  return {
    id: "view-code",
    label: "View code",
    ...(shortcut !== undefined ? { shortcut } : {}),
    disabled: source.target === null,
    ...(source.reason !== undefined ? { hint: source.reason } : {}),
    onSelect: () => {
      viewCode(target, selection);
    },
  };
}

/**
 * Add proposed step after… for a step a proposed step can follow (none otherwise): one entry, or a
 * submenu of a decision's actual paths. It creates the proposal, selects it, and focuses its title.
 */
function proposeEntries(target: PageTarget, choice: ProposalChoice | null, author: string | null): MenuEntry[] {
  if (!choice) {
    return [];
  }
  const add = (path?: ProposalPath) => () => {
    if (author !== null && proposeStep(target, choice, path, author) !== undefined) {
      showInSelected(PROPOSAL_TITLE_FIELD);
    }
  };
  const waiting = author === null ? { disabled: true, hint: "Loading who you are…" } : {};
  if (choice.paths.length > 0) {
    return [
      {
        id: "propose",
        label: "Add proposed step after",
        ...waiting,
        submenu: choice.paths.map((path) => ({ id: `propose-${path}`, label: `${PATH_WORDS[path]}…`, onSelect: add(path) })),
      },
    ];
  }
  return [{ id: "propose", label: "Add proposed step after…", ...waiting, onSelect: add() }];
}

/** The menu of a Flow step or a run of steps: a run's heading, View code, Combine…, and Add proposed step after…. */
function stepEntries(target: PageTarget, selection: Selection, functionName: string, author: string | null): MenuEntry[] {
  const choice = combineOf(target, selection);
  const run = "run" in choice ? choice.run : undefined;
  const lines = run ? (run.startLine === run.endLine ? `line ${run.startLine}` : `lines ${run.startLine}–${run.endLine}`) : "";
  return [
    ...(run ? [{ kind: "heading", id: "run", label: `${run.steps.length} steps · ${functionName} · ${lines}` } satisfies MenuEntry] : []),
    viewCodeEntry(target, selection),
    {
      id: "combine",
      label: "Combine…",
      disabled: "reason" in choice,
      ...("reason" in choice ? { hint: choice.reason } : {}),
      onSelect: () => {
        if ("members" in choice) {
          openDialog({ kind: "groupFromLines", target, cardId: choice.cardId, members: choice.members });
        }
      },
    },
    ...proposeEntries(target, proposalChoiceOf(target, selection), author),
  ];
}

/**
 * The context menus of one card. `project` is null for a card of a project that isn't connected;
 * `source` (once the card's model loaded) offers Open source and Add as entrance; `block` offers Show
 * callers; `name` names the function in a run's heading.
 */
export function useCardMenu(
  target: PageTarget,
  project: ProjectId | null,
  cardId: string,
  folded: boolean,
  block: boolean,
  source: CardSource | null,
  name = "",
): CardMenu {
  const menu = useContextMenu("Card actions", 220);
  const author = useAuthor();
  const own: Selection = { kind: "card", cardId, key: null };
  const cardEntries = (): MenuEntry[] => [
    {
      id: "source",
      label: "Open source",
      icon: "external-link",
      disabled: sourceOf(target, own).target === null,
      onSelect: () => {
        viewCode(target, own);
      },
    },
    ...(block
      ? [
          {
            id: "callers",
            label: "Show callers",
            onSelect: () => void showCallers(target, cardId),
          },
        ]
      : []),
    {
      id: "link",
      label: "Draw link…",
      icon: "link",
      onSelect: () => {
        setLinking(target, cardId);
      },
    },
    ...(project !== null && source?.definition && canBeHandler(source.definition.kind)
      ? [
          {
            id: "add-entrance",
            label: "Add as entrance…",
            icon: "entrance",
            onSelect: () => {
              if (source.definition) {
                openDialog({ kind: "launchEditor", project, mode: "add", intent: "handler", handler: source.definition });
              }
            },
          } satisfies MenuEntry,
        ]
      : []),
    { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, cardId) },
    { kind: "separator", id: "s1" },
    {
      id: "fold",
      label: folded ? "Unfold card" : "Fold card",
      onSelect: () => {
        editPage(target, (page) => {
          setCardsFolded(page, [cardId], !folded);
        });
      },
    },
    { kind: "separator", id: "s2" },
    {
      id: "remove",
      label: "Remove from page",
      danger: true,
      onSelect: () => {
        removeCard(target, cardId);
      },
    },
  ];
  const groupEntries = (groupId: string): MenuEntry[] => {
    const board = getOpenBoard(target.board);
    const group = board ? findPage(board.document, target.page)?.groups.find((item) => item.id === groupId) : undefined;
    if (!group) {
      return [];
    }
    return [
      viewCodeEntry(target, { kind: "group", groupId }),
      {
        id: "collapse",
        label: group.collapsed ? "Expand group" : "Collapse group",
        onSelect: () => {
          editGroup(target, groupId, { collapsed: !group.collapsed });
        },
      },
      {
        id: "describe",
        label: "Describe group…",
        onSelect: () => {
          select(target, { kind: "group", groupId });
          showInSelected(`selected.group:${groupId}`);
        },
      },
      { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, groupId) },
      { kind: "separator", id: "s1" },
      {
        id: "ungroup",
        label: "Ungroup",
        danger: true,
        onSelect: () => {
          ungroup(target, groupId);
        },
      },
    ];
  };
  return {
    onContextMenu: (event) => {
      event.stopPropagation();
      select(target, { kind: "card", cardId, key: null });
      menu.open(event, cardEntries());
    },
    openGroupMenu: (groupId, at) => {
      const entries = groupEntries(groupId);
      if (entries.length > 0) {
        menu.open(at, entries);
      }
    },
    openStepMenu: (at) => {
      menu.open(at, stepEntries(target, getPageView(target).selection, name, author));
    },
    // The menu renders in a portal, but its events still bubble through the card: a choice never
    // also clicks (selects) or double-clicks (folds) the card it was opened on.
    element: menu.element ? (
      <span className="card-menu-layer" onClick={stop} onDoubleClick={stop} onContextMenu={stop}>
        {menu.element}
      </span>
    ) : null,
  };
}
