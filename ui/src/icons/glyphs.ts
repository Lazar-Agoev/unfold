/**
 * The icon set's drawings. Path data is the designs' own, verbatim, in the viewBox each was drawn in:
 * the custom 16px icons of Home, the 24px icons of the board, Add, the peek sheet, and Publish, and the
 * 10px disclosure chevrons. The Lucide-style icons of Project settings and Health (check, close, folder,
 * warning, info) are redrawn in the 16px style. Strokes are `currentColor`, 1.5 unless noted, with
 * round caps and joins.
 */

/** One shape of an icon. Filled shapes (the dots of `dots`, `info`) have no stroke. */
export type Shape =
  | { readonly d: string; readonly dash?: string }
  | { readonly circle: readonly [cx: number, cy: number, r: number]; readonly filled?: true }
  | { readonly rect: readonly [x: number, y: number, width: number, height: number, rx: number]; readonly dash?: string };

export interface Glyph {
  readonly box: 10 | 16 | 24;
  readonly stroke?: number;
  readonly shapes: readonly Shape[];
}

export const ICON_NAMES = [
  "home",
  "chevron-left",
  "chevron-right",
  "chevron-down",
  "chevron-right-small",
  "chevron-down-small",
  "dots",
  "plus",
  "page",
  "entrance",
  "group",
  "link",
  "search",
  "clock",
  "drafts",
  "folder",
  "board",
  "board-draft",
  "branch",
  "check",
  "gear",
  "arrange",
  "external-link",
  "reveal",
  "pin",
  "close",
  "add-circle",
  "warning-circle",
  "warning-triangle",
  "info",
  "shield",
  "publish",
  "move-back",
  "copy",
  "activity",
  "edit",
  "layers",
  "minus",
  "back",
  "forward",
  "agent",
] as const;

export type IconName = (typeof ICON_NAMES)[number];

const FOLDER = "M2 4.5C2 3.67 2.67 3 3.5 3H6.5L8 4.5H12.5C13.33 4.5 14 5.17 14 6V11.5C14 12.33 13.33 13 12.5 13H3.5C2.67 13 2 12.33 2 11.5V4.5Z";

export const GLYPHS: Readonly<Record<IconName, Glyph>> = {
  // Window tabs.
  home: { box: 16, stroke: 1.4, shapes: [{ d: "M2.5 7L8 2.5L13.5 7V13.5H9.5V10H6.5V13.5H2.5V7Z" }] },
  // Board breadcrumb, dock, menus.
  "chevron-left": { box: 24, shapes: [{ d: "M15 18l-6-6 6-6" }] },
  "chevron-right": { box: 24, shapes: [{ d: "M9 18l6-6-6-6" }] },
  "chevron-down": { box: 24, shapes: [{ d: "M6 9l6 6 6-6" }] },
  // Sidebar disclosure.
  "chevron-right-small": { box: 10, shapes: [{ d: "M3.5 2L6.5 5L3.5 8" }] },
  "chevron-down-small": { box: 10, shapes: [{ d: "M2 3.5L5 6.5L8 3.5" }] },
  // The filled-dots `⋯` everywhere (as on the project menu).
  dots: {
    box: 16,
    shapes: [
      { circle: [3.5, 8, 1.25], filled: true },
      { circle: [8, 8, 1.25], filled: true },
      { circle: [12.5, 8, 1.25], filled: true },
    ],
  },
  plus: { box: 16, shapes: [{ d: "M8 3.5V12.5M3.5 8H12.5" }] },
  minus: { box: 16, shapes: [{ d: "M3.5 8H12.5" }] },
  // Pages and On this page.
  page: { box: 24, shapes: [{ d: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" }, { d: "M14 3v5h5" }] },
  entrance: {
    box: 24,
    shapes: [{ d: "M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" }, { d: "M10 17l5-5-5-5" }, { d: "M15 12H3" }],
  },
  group: { box: 24, shapes: [{ d: "M13 4H9v16h4" }] },
  link: { box: 16, shapes: [{ d: "M2 8h9", dash: "2.5 2" }, { d: "M10 5l3 3-3 3" }] },
  // Home sidebar.
  search: { box: 16, shapes: [{ circle: [7, 7, 4.5] }, { d: "M10.5 10.5L13.5 13.5" }] },
  clock: { box: 16, shapes: [{ circle: [8, 8, 5.5] }, { d: "M8 5V8L10 9.5" }] },
  drafts: { box: 16, shapes: [{ rect: [3, 2.5, 10, 11, 1.5], dash: "2 2" }] },
  folder: { box: 16, shapes: [{ d: FOLDER }] },
  board: { box: 16, shapes: [{ d: "M5.5 2.5H11.5C12.6 2.5 13.5 3.4 13.5 4.5V10.5" }, { rect: [2.5, 4.75, 8.75, 8.75, 1.5] }] },
  "board-draft": {
    box: 16,
    shapes: [
      { d: "M5.5 2.5H11.5C12.6 2.5 13.5 3.4 13.5 4.5V10.5", dash: "2 2" },
      { rect: [2.5, 4.75, 8.75, 8.75, 1.5], dash: "2 1.75" },
    ],
  },
  // Home's branch switcher.
  branch: {
    box: 16,
    shapes: [
      { circle: [4.5, 3.5, 1.75] },
      { circle: [4.5, 12.5, 1.75] },
      { circle: [11.5, 5.5, 1.75] },
      { d: "M4.5 5.25V10.75M11.5 7.25C11.5 9.5 8 9.5 5.5 11" },
    ],
  },
  // Home's Health strip; also Project settings' badge check, redrawn.
  check: { box: 16, shapes: [{ d: "M3.5 8.5L6.5 11.5L12.5 4.5" }] },
  gear: {
    box: 16,
    shapes: [
      { circle: [8, 8, 2] },
      { d: "M8 1.8V3.6M8 12.4V14.2M14.2 8H12.4M3.6 8H1.8M12.4 3.6L11.1 4.9M4.9 11.1L3.6 12.4M12.4 12.4L11.1 11.1M4.9 4.9L3.6 3.6" },
    ],
  },
  // Dock.
  arrange: {
    box: 24,
    shapes: [
      { rect: [3, 9, 6, 6, 1.5] },
      { rect: [15, 3, 6, 6, 1.5] },
      { rect: [15, 15, 6, 6, 1.5] },
      { d: "M9 12h3M12 6v12M12 6h3M12 18h3" },
    ],
  },
  // Peek sheet menu and header.
  "external-link": {
    box: 24,
    shapes: [{ d: "M15 3h6v6" }, { d: "M10 14 21 3" }, { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" }],
  },
  // Reveal in Finder: the 16px folder, opened.
  reveal: {
    box: 16,
    shapes: [
      { d: "M2 12V4.5C2 3.67 2.67 3 3.5 3H6.5L8 4.5H12C12.83 4.5 13.5 5.17 13.5 6V7.25" },
      { d: "M2 12L3.6 8.2C3.8 7.75 4.25 7.5 4.75 7.5H13.6C14.3 7.5 14.75 8.2 14.5 8.85L13.3 12.2C13.1 12.7 12.6 13 12.1 13H3C2.45 13 2 12.55 2 12Z" },
    ],
  },
  pin: {
    box: 24,
    shapes: [
      { d: "M12 17v5" },
      {
        d: "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z",
      },
    ],
  },
  // Project settings' close, redrawn.
  close: { box: 16, shapes: [{ d: "M4 4L12 12M12 4L4 12" }] },
  // The peek's ⊕.
  "add-circle": { box: 24, stroke: 1.75, shapes: [{ circle: [12, 12, 9] }, { d: "M12 8v8" }, { d: "M8 12h8" }] },
  // Health's problem icon and Publish's conflict icon, redrawn; the triangle follows the failed state glyph.
  "warning-circle": { box: 16, shapes: [{ circle: [8, 8, 6] }, { d: "M8 5V8.75" }, { circle: [8, 11, 0.9], filled: true }] },
  "warning-triangle": {
    box: 16,
    shapes: [
      { d: "M7.13 2.5C7.52 1.83 8.48 1.83 8.87 2.5L14.1 11.5C14.48 12.17 14 13 13.23 13H2.77C2 13 1.52 12.17 1.9 11.5L7.13 2.5Z" },
      { d: "M8 6V8.75" },
      { circle: [8, 10.9, 0.8], filled: true },
    ],
  },
  // The state line's fallback note; also Health's quiet note.
  info: { box: 16, shapes: [{ circle: [8, 8, 6] }, { d: "M8 7.25V11" }, { circle: [8, 5, 0.9], filled: true }] },
  // The first-Compile promise.
  shield: { box: 16, shapes: [{ d: "M8 1.75L13 3.75V7.5C13 10.6 10.9 13.1 8 14.25C5.1 13.1 3 10.6 3 7.5V3.75L8 1.75Z" }] },
  // Add's commands and the Publish changes list.
  publish: { box: 24, shapes: [{ d: "M12 15V3" }, { d: "m7 8 5-5 5 5" }, { d: "M5 15v4a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-4" }] },
  "move-back": { box: 24, shapes: [{ d: "M9 14 4 9l5-5" }, { d: "M4 9h10.5a5.5 5.5 0 0 1 0 11H11" }] },
  copy: { box: 24, shapes: [{ rect: [8, 8, 13, 13, 2] }, { d: "M4 16V5a2 2 0 0 1 2-2h11" }] },
  activity: { box: 24, shapes: [{ d: "M22 12h-4l-3 8L9 4l-3 8H2" }] },
  edit: { box: 24, shapes: [{ d: "M12 20h9" }, { d: "M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" }] },
  // Add's "All projects".
  layers: { box: 24, shapes: [{ d: "m12 3 9 5-9 5-9-5 9-5Z" }, { d: "m3 13 9 5 9-5" }] },
  // Go back and forward (the navigation commands), as arrows.
  back: { box: 16, shapes: [{ d: "M13 8H3.5M7.5 4L3.5 8L7.5 12" }] },
  forward: { box: 16, shapes: [{ d: "M3 8H12.5M8.5 4L12.5 8L8.5 12" }] },
  // The agent mark beside an agent's name: a small four-point spark.
  agent: { box: 10, stroke: 1.2, shapes: [{ d: "M5 1.5L5.9 4.1L8.5 5L5.9 5.9L5 8.5L4.1 5.9L1.5 5L4.1 4.1Z" }] },
};
