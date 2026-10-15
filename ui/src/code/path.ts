/**
 * The code panel's path, shortened to fit its row (docs/ui-shell.md#the-code-panel): in the middle
 * first, then the file's folders, then the definition, and only last the selection's label.
 */

const graphemes = new Intl.Segmenter();

/** How many characters `text` shows; in the code font, each is as wide as any other. */
export function letters(text: string): number {
  return [...graphemes.segment(text)].length;
}

/**
 * A file path's forms: whole, then shortened in the middle so its name stays (`src/…/orders.ts`, then
 * `…/orders.ts`), each shorter than the one before.
 */
export function fileForms(path: string): string[] {
  const parts = path.split("/");
  const forms = [path];
  if (parts.length > 1) {
    const name = parts.at(-1) ?? path;
    for (const form of [`${parts[0] ?? ""}/…/${name}`, `…/${name}`]) {
      if (letters(form) < letters(forms.at(-1) ?? path)) {
        forms.push(form);
      }
    }
  }
  return forms;
}

/** A path's widths, in pixels. */
export interface PathWidths {
  /** The file in each of its forms (`fileForms`). */
  readonly file: readonly number[];
  /** Each step after the file, its separator's included. */
  readonly steps: readonly number[];
  /** The `› …` drawn for each run of steps left out. */
  readonly gap: number;
}

/** How a path is shortened: the steps it leaves out (by their place in the path), and the form its file shows. */
export interface Shortened {
  readonly out: ReadonlySet<number>;
  readonly form: number;
}

/**
 * How a path gives way to fit `room` pixels, stopping as soon as it fits: first the steps strictly
 * between the definition (the step after the file) and the selection, deepest first; then the file's
 * folders; then the definition. The step showing and the selection never go; past all of that, the
 * selection's label shortens. The file shows its longest form that fits.
 */
export function shortenPath(widths: PathWidths, showing: number, room: number): Shortened {
  const out = new Set<number>();
  // Each run of steps left out draws one `› …`, where its first step was.
  const width = (form: number) => (widths.file[form] ?? 0) + widths.steps.reduce((sum, step, at) => sum + (!out.has(at + 1) ? step : out.has(at) ? 0 : widths.gap), 0);
  const formThatFits = () => widths.file.findIndex((_, form) => width(form) <= room);
  const last = widths.steps.length;
  for (let index = last - 1; index >= 2 && width(0) > room; index -= 1) {
    if (index !== showing) {
      out.add(index);
    }
  }
  if (formThatFits() === -1 && last >= 2 && showing !== 1) {
    out.add(1);
  }
  const form = formThatFits();
  return { out, form: form === -1 ? widths.file.length - 1 : form };
}
