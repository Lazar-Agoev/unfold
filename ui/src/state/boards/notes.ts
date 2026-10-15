/**
 * Pure edits of a page's notes: short plain text written on the page, free (at a canvas
 * position) or attached to a card, frame, resource card, or link (at an offset from it, so it moves
 * with it). Notes aren't signed and have no health. Each function is an Immer recipe over a draft
 * page; an empty list is deleted, as the file omits it.
 */

import type { Note, Page } from "../../ipc/generated";
import { uniqueId } from "./document";

/** A page's notes, as the queries read them. */
interface Noted {
  readonly notes?: readonly Note[] | undefined;
}

export function findNote(page: Noted, noteId: string): Note | undefined {
  return (page.notes ?? []).find((note) => note.id === noteId);
}

/** What a new note is: its text, its place (an offset from `attach`, else a canvas position), and who wrote it. */
export interface NewNote {
  /** An id of its own choosing (the save refuses one the page has); absent: one of its own. */
  readonly id?: string;
  readonly text: string;
  readonly at: readonly [number, number];
  readonly attach?: string;
  readonly author: string;
}

/** Adds a note and returns its id. Positions are rounded: the file holds finite numbers. */
export function addNote(page: Page, note: NewNote): string {
  const id = note.id ?? uniqueId(page, "note");
  page.notes ??= [];
  page.notes.push({
    id,
    text: note.text,
    at: [Math.round(note.at[0]), Math.round(note.at[1])],
    ...(note.attach !== undefined ? { attach: note.attach } : {}),
    author: note.author,
  });
  return id;
}

/** Rewrites a note's text, trimmed, which makes `author` its writer; blank text changes nothing. The board's save checks it. */
export function setNoteText(page: Page, noteId: string, text: string, author: string): void {
  const note = findNote(page, noteId);
  const written = text.trim();
  if (note && written.length > 0 && written !== note.text) {
    note.text = written;
    note.author = author;
  }
}

/** Places a note: an offset from `attach`, or a canvas position (null: free). */
export function placeNote(page: Page, noteId: string, at: readonly [number, number], attach: string | null): void {
  const note = findNote(page, noteId);
  if (!note || !Number.isFinite(at[0]) || !Number.isFinite(at[1])) {
    return;
  }
  note.at = [Math.round(at[0]), Math.round(at[1])];
  if (attach === null) {
    delete note.attach;
  } else {
    note.attach = attach;
  }
}
