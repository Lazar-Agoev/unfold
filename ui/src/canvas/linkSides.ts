/**
 * The sides of a card a line leaves or enters by (each card has a connection handle on each, for React
 * Flow to attach to); the page's router picks among them (`lines/route.ts`).
 */

export const LINK_SIDES = ["left", "right", "top", "bottom"] as const;
