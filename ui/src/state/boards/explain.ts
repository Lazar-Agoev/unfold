/**
 * What people make by hand on a page, under the rules agents follow (docs/ui-shell.md#explaining-by-hand):
 * combined steps, proposed steps, resource cards, drawn links, a connection's kind, and
 * a relationship's columns and cardinality. Each is saved at once as one undo step (`saveStep`), so the board's own
 * checks decide: what they refuse leaves the page as it was and comes back as a message. Nothing here
 * re-checks a rule the board checks; what each block allows comes from the drawn flow's actions.
 */

import { produce, type Draft } from "immer";
import type { Cardinality, ConnectionKind, Page } from "../../ipc/generated";
import { addConnection, claimOf, setConnectionClaim, setConnectionKind } from "./connections";
import { addCombined, type NewCombined } from "./document";
import { addProposal, type NewProposal } from "./proposals";
import { addResource, type NewResource } from "./resources";
import { saveStep } from "./store";
import type { PageTarget } from "./types";

/** How a hand edit ended: what it made, or the board's reason for refusing it. */
type HandOutcome<T> = { readonly ok: true; readonly value: T } | { readonly ok: false; readonly message: string };

/** Makes one change to the target's page and saves it at once, as one undo step named `name`. */
async function handStep<T>(target: PageTarget, name: string, recipe: (page: Draft<Page>) => T): Promise<HandOutcome<T>> {
  const outcome = await saveStep(target.board, target.page, name, (document) => {
    let value: T | undefined;
    const next = produce(document, (draft) => {
      const page = draft.pages.find((item) => item.id === target.page);
      if (!page) {
        throw new Error("The page is gone.");
      }
      value = recipe(page);
    });
    return { document: next, value: value as T };
  });
  return outcome.ok ? { ok: true, value: outcome.value } : { ok: false, message: outcome.error.message };
}

/** Combines blocks under a title, collapsed; returns the combined step's id. */
export function combineSteps(target: PageTarget, combined: NewCombined): Promise<HandOutcome<string>> {
  return handStep(target, "Combine steps", (page) => addCombined(page, combined));
}

/** Proposes a step after a block or at the start of a body; returns its id. */
export function proposeStep(target: PageTarget, proposal: NewProposal): Promise<HandOutcome<string>> {
  return handStep(target, "Propose a step", (page) => addProposal(page, proposal));
}

/** Adds a resource card; returns its id. A person's needs no citation. */
export function addResourceCard(target: PageTarget, resource: NewResource): Promise<HandOutcome<string>> {
  return handStep(target, "Add resource", (page) => addResource(page, resource));
}

/** Draws an `other` connection from one item to another, saying what it is in `label`; returns its id. A person's needs no step and no citation. */
export function drawLink(target: PageTarget, link: { readonly from: string; readonly to: string; readonly label: string; readonly author: string }): Promise<HandOutcome<string>> {
  return handStep(target, "Draw link", (page) => {
    const id = addConnection(page, { kind: "other", from: link.from, to: link.to, anchors: [], author: link.author });
    const drawn = page.links.find((item) => item.id === id);
    if (drawn) {
      drawn.label = { text: link.label, author: link.author, signature: "" };
    }
    return id;
  });
}

/** Gives a connection another kind, which makes `author` the one who drew it. */
export function setLinkKind(target: PageTarget, linkId: string, kind: ConnectionKind, author: string): Promise<HandOutcome<null>> {
  return handStep(target, "Change kind", (page) => {
    setConnectionKind(page, linkId, kind, author);
    return null;
  });
}

/** A relationship's columns and cardinality, as Selected sets them (absent: unchanged). */
export interface RelationshipChange {
  readonly fromColumn?: string;
  readonly toColumn?: string;
  readonly cardinality?: Cardinality;
}

/** Sets a relationship's columns or cardinality, which makes `author` the one who drew it. */
export function setRelationship(target: PageTarget, linkId: string, change: RelationshipChange, author: string): Promise<HandOutcome<null>> {
  return handStep(target, "Edit relationship", (page) => {
    const link = page.links.find((item) => item.id === linkId);
    if (link) {
      setConnectionClaim(page, linkId, { ...claimOf(link), kind: link.kind, ...change }, author);
    }
    return null;
  });
}
