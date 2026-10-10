/**
 * A card whose project isn't connected on this machine: where the card would be, which project and
 * definition it shows, who added it, and Connect project (the Add project picker). Links still attach
 * to it; it can be selected, moved, and removed like any card.
 */

import { Button, Monogram } from "../../components";
import type { Card } from "../../ipc/generated";
import { authorName } from "../../code/text";
import { cardKey, keyDisplayName, type PageTarget } from "../../state/boards";
import { addProjectFromPicker } from "../../state/workspace";
import { useCardMenu } from "./cardMenu";
import { CardHandles } from "./parts";
import { clickCard } from "./select";

/** The placeholder's width. */
const PLACEHOLDER_WIDTH = 288;

export interface PlaceholderCardProps {
  readonly target: PageTarget;
  readonly card: Card;
  /** The shared name of the project the card is from. */
  readonly projectName: string;
  readonly selected: boolean;
}

export function PlaceholderCard({ target, card, projectName, selected }: PlaceholderCardProps) {
  const key = cardKey(card);
  const name = keyDisplayName(key);
  const block = "block" in card.ref;
  // No project to read from: the menu offers what needs none (draw a link, remove).
  const menu = useCardMenu(target, null, card.id, card.folded === true, false, null);
  const author = card.description?.author;
  return (
    <article
      className={`board-card placeholder-card card-drag${selected ? " selected" : ""}`}
      data-card-id={card.id}
      data-key={key}
      style={{ width: PLACEHOLDER_WIDTH }}
      aria-label={`${name}, from ${projectName}, not connected`}
      onClick={() => {
        clickCard(target, card.id, null);
      }}
      onContextMenu={menu.onContextMenu}
    >
      <CardHandles />
      <div className="placeholder-from">
        <Monogram name={projectName} size={20} />
        <span>From {projectName} · not connected</span>
      </div>
      <p className="placeholder-name">{block ? `ƒ ${name}` : name}</p>
      <p className="placeholder-note">
        {author !== undefined
          ? `${authorName(author)} added this card. Connect the project to read it.`
          : `This card is from ${projectName}. Connect the project to read it.`}
      </p>
      <Button
        variant="secondary"
        className="placeholder-connect nodrag"
        onClick={(event) => {
          event.stopPropagation();
          void addProjectFromPicker();
        }}
      >
        Connect project
      </Button>
      {menu.element}
    </article>
  );
}
