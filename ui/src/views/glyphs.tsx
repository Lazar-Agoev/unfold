/** Small glyphs the views share: the definition kind mark (`ƒ`, `C`) and the language tag. */

import type { Language } from "../ipc/generated";
import "./views.css";

const CALLABLE = new Set(["function", "method", "constructor", "getter", "setter"]);

/** `ƒ` for functions and methods, `C` for classes, `v` for variables; a dot otherwise. */
export function KindGlyph({ kind }: { readonly kind: string }) {
  const glyph = CALLABLE.has(kind) ? "ƒ" : kind === "class" ? "C" : kind === "variable" ? "v" : "·";
  return (
    <span className={`kind-glyph kind-glyph-${CALLABLE.has(kind) ? "callable" : kind}`} aria-hidden="true">
      {glyph}
    </span>
  );
}

const LANGUAGE_TAGS: Readonly<Record<Language, string>> = { typescript: "ts", python: "py" };

/** `ts` or `py`, for mixed repositories. */
export function LanguageTag({ language }: { readonly language: Language }) {
  return (
    <span className="language-tag" title={language === "typescript" ? "TypeScript" : "Python"}>
      {LANGUAGE_TAGS[language]}
    </span>
  );
}
