/** How an entrance card names itself: its title, what its card shows (which its width follows), and its targets' heading. */

import type { CardView, TargetView } from "../ipc/generated";
import type { EntranceWidthInput } from "./sizing";

/** The entrance's title as the trail and a new board show it: its label, or `POST /orders`. */
export function entranceTitle(card: Extract<CardView, { type: "entrance" }>): string {
  const { header } = card;
  if (header.label !== null && header.label.length > 0) {
    return header.label;
  }
  if (header.method !== null && header.trigger !== null) {
    return `${header.method} ${header.trigger}`;
  }
  return header.trigger ?? header.title;
}

/** What an entrance card shows: `POST` and `/orders` when it has both, else its title; where it is declared; its handlers. */
export function entranceCardText(card: Extract<CardView, { type: "entrance" }>): EntranceWidthInput {
  const { header, evidence } = card;
  const method = header.method !== null && header.trigger !== null ? header.method : null;
  return {
    method,
    title: method !== null ? (header.trigger ?? entranceTitle(card)) : entranceTitle(card),
    location: `${evidence.path}${evidence.startLine !== null ? `:${evidence.startLine}` : ""}`,
    handlers: card.targets.map((handler) => handler.name ?? handler.text),
  };
}

/**
 * The heading over an entrance's targets, from the roles they were returned with: "Starts" when every
 * target is a launch's startup, else "Handlers, in order".
 */
export function targetsHeading(targets: readonly TargetView[]): string {
  return targets.length > 0 && targets.every((target) => target.role === "startup") ? "Starts" : "Handlers, in order";
}
