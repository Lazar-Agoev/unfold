/**
 * Pure edits of a page's notes (design §4.8): short plain text written on the page, free (at a canvas
 * position) or attached to a card, frame, resource card, or link (at an offset from it, so it moves
 * with it). Notes aren't signed and have no health. Each function is an Immer recipe over a draft
 * page; an empty list is deleted, as the file omits it.
 */

import type { Note, Page } from "../../ipc/generated";
import { uniqueId } from "./document";

/** The most characters a note holds (Unicode scalar values, as the file counts them). */
export const NOTE_MAX = 400;

/** A page's notes, as the queries read them. */
interface Noted {
  readonly notes?: readonly Note[] | undefined;
}

export function findNote(page: Noted, noteId: string): Note | undefined {
  return (page.notes ?? []).find((note) => note.id === noteId);
}

/** How long a note is, as the file counts it: in Unicode scalar values (a surrogate pair is one). */
export function noteLength(text: string): number {
  return text.replace(/[\uD800-\uDBFF][\uDC00-\uDFFF]/g, "_").length;
}

/** A note's text as the file holds it: trimmed, or null when it is blank or longer than `NOTE_MAX`. */
export function noteText(text: string): string | null {
  const trimmed = text.trim();
  return trimmed.length > 0 && noteLength(trimmed) <= NOTE_MAX ? trimmed : null;
}

/** What a new note is: its text, its place (an offset from `attach`, else a canvas position), and who wrote it. */
export interface NewNote {
  /** A free id (`isFreeId`); absent: one of its own. */
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

/** Rewrites a note's text, which makes `author` its writer; blank or too long text changes nothing. */
export function setNoteText(page: Page, noteId: string, text: string, author: string): void {
  const note = findNote(page, noteId);
  const written = noteText(text);
  if (note && written !== null && written !== note.text) {
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
