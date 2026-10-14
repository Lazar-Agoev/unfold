/**
 * `add_notes` (design §6.7): adds notes to a page, or updates them by id: short plain text, attached to
 * a card, frame, resource card, or link (so it moves with it) or free. A new note goes beside what it is
 * attached to, or beside the item it is `near`, else right of everything, covering nothing. A note a
 * person wrote is theirs unless the call says the person asked.
 */

import type { AddNotesOp, AgentItemResult, AgentItemStatus, AgentNoteResult, AgentNoteSpec } from "@unfold/desktop/agent-api";
import { attachPoint } from "../../../canvas/frames/geometry";
import { noteSize } from "../../../canvas/sizing";
import { measureText } from "../../../canvas/measure";
import type { Note, Page } from "../../../ipc/generated";
import { findCard, findCombined, isFreeId, isItemId } from "../document";
import { findFrame } from "../frames";
import { NOTE_MAX, addNote, findNote, noteText, placeNote, setNoteText } from "../notes";
import { findResource } from "../resources";
import { guardPeopleText, invalid, noteName, pageRects, type OpContext } from "./context";
import { freeNoteSpot, noteSpot } from "./place";

/** A note's result entry. */
export function noteResult(note: Note, status: AgentItemStatus, placed: boolean): AgentNoteResult {
  return {
    kind: "note",
    id: note.id,
    status,
    name: noteName(note),
    ...(note.attach !== undefined ? { attach: note.attach } : {}),
    ...(placed ? { at: [note.at[0], note.at[1]] as const } : {}),
  };
}

/** Refuses an item a note can't be attached to. */
function checkAttach(page: Page, id: string): void {
  if (findCard(page, id) || findFrame(page, id) || findResource(page, id) || page.links.some((link) => link.id === id)) {
    return;
  }
  throw invalid(
    findCombined(page, id) || findNote(page, id)
      ? `A note attaches to a card, a frame, a resource card, or a connection; “${id}” is a ${findNote(page, id) ? "note" : "combined step"}. Attach it to a card instead.`
      : `There is no card, frame, resource card, or connection with id “${id}” on this page to attach the note to. read_page lists the page's ids.`,
  );
}

/** The note a spec updates: by id (`attach` absent keeps where it is), else a note with the same text on the same item; undefined for a new one. */
function existingFor(page: Page, spec: AgentNoteSpec, text: string): Note | undefined {
  if (spec.id === undefined) {
    return (page.notes ?? []).find((note) => note.text === text && note.attach === spec.attach);
  }
  const note = findNote(page, spec.id);
  if (!note && !isFreeId(page, spec.id)) {
    throw invalid(
      isItemId(spec.id)
        ? `“${spec.id}” is the id of another item on this page (not a note). Give the note another id, or leave id out.`
        : `“${spec.id}” can't name a note: an id is letters, digits, “_”, “.”, and “-”, starting with a letter or digit.`,
    );
  }
  return note;
}

/**
 * Where a note goes: beside what it is attached to (as an offset from it), else beside `near`, else
 * right of everything.
 */
function notePlace(ctx: OpContext, page: Page, spec: AgentNoteSpec, text: string): readonly [number, number] {
  const size = noteSize(text, measureText);
  if (spec.attach === undefined) {
    return spec.near !== undefined ? noteSpot(ctx, page, size, spec.near) : freeNoteSpot(ctx, page, size);
  }
  const [x, y] = noteSpot(ctx, page, size, spec.attach);
  const from = attachPoint(page, pageRects(ctx, page), spec.attach) ?? { x: 0, y: 0 };
  return [x - from.x, y - from.y];
}

/** Adds or updates one note; returns its result. */
function applyNoteSpec(ctx: OpContext, page: Page, spec: AgentNoteSpec, replacePeopleText: boolean | undefined): AgentNoteResult {
  const text = noteText(spec.text);
  if (text === null) {
    throw invalid(`A note is plain text of 1 to ${NOTE_MAX} characters: two sentences at most, for what a reader would otherwise miss.`);
  }
  if (spec.attach !== undefined) {
    checkAttach(page, spec.attach);
  }
  const existing = existingFor(page, spec, text);
  const author = ctx.agent.author;
  if (!existing) {
    const at = notePlace(ctx, page, spec, text);
    const id = addNote(page, { text, at, author, ...(spec.attach !== undefined ? { attach: spec.attach } : {}), ...(spec.id !== undefined ? { id: spec.id } : {}) });
    const note = findNote(page, id);
    if (!note) {
      throw invalid("The note couldn't be added.");
    }
    return noteResult(note, "created", true);
  }
  if (existing.text !== text) {
    guardPeopleText(`the note “${noteName(existing)}”`, existing.author, replacePeopleText);
  }
  const before = JSON.stringify(existing);
  setNoteText(page, existing.id, text, author);
  // Without attach, an update leaves the note where it is attached (or free).
  const attach = spec.attach ?? existing.attach;
  const moved = attach !== existing.attach || spec.near !== undefined;
  if (moved) {
    placeNote(page, existing.id, notePlace(ctx, page, { ...spec, ...(attach !== undefined ? { attach } : {}) }, text), attach ?? null);
  }
  const note = findNote(page, existing.id) ?? existing;
  return noteResult(note, JSON.stringify(note) === before ? "kept" : "updated", moved);
}

export function applyAddNotes(page: Page, op: AddNotesOp, ctx: OpContext): AgentItemResult[] {
  return op.notes.map((spec): AgentItemResult => applyNoteSpec(ctx, page, spec, op.replacePeopleText));
}
