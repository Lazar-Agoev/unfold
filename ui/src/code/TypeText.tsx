/**
 * A written type (`TypeText`, docs/reader.md): its text with one span per type name
 * in it. A `declared` name is underlined and opens its definition (`onToggle`); an `external` one
 * has a dotted underline whose tooltip names its package and never opens; a `builtin` one reads like
 * punctuation; an `unresolved` one has a dashed underline whose tooltip says why. Between names,
 * words read as text and punctuation (`[]`, `<>`, `|`) in the contour gray.
 */

import { Tooltip } from "../components";
import type { Key, TypeSpan, TypeText as TypeTextModel } from "../ipc/generated";
import "./types.css";

export type TypeSegment =
  | { readonly kind: "word" | "punct"; readonly text: string }
  | { readonly kind: "span"; readonly text: string; readonly span: TypeSpan; readonly index: number };

const WORD = /[\p{L}\p{N}_$.]+|[^\p{L}\p{N}_$.]+/gu;
const WORD_START = /^[\p{L}\p{N}_$]/u;

function plain(text: string): TypeSegment[] {
  return [...text.matchAll(WORD)].map((match) => ({ kind: WORD_START.test(match[0]) ? ("word" as const) : ("punct" as const), text: match[0] }));
}

/** The type's text cut at its spans (UTF-16 offsets, as JavaScript strings index). */
export function typeSegments(type: TypeTextModel): TypeSegment[] {
  const segments: TypeSegment[] = [];
  let at = 0;
  const spans = type.spans.map((span, index) => ({ span, index })).sort((a, b) => a.span.start - b.span.start);
  for (const { span, index } of spans) {
    if (span.start < at || span.end > type.text.length || span.end <= span.start) {
      continue;
    }
    segments.push(...plain(type.text.slice(at, span.start)));
    segments.push({ kind: "span", text: type.text.slice(span.start, span.end), span, index });
    at = span.end;
  }
  segments.push(...plain(type.text.slice(at)));
  return segments;
}

export interface TypeTextProps {
  readonly type: TypeTextModel;
  /** The index of the span whose definition is open under this row. */
  readonly open?: number | null;
  /** Opens or closes a `declared` span's definition; without it, declared names read as plain names. */
  readonly onToggle?: (index: number, target: Key) => void;
}

function SpanText({ segment, open, onToggle }: { readonly segment: Extract<TypeSegment, { kind: "span" }> } & Omit<TypeTextProps, "type">) {
  const { span, text, index } = segment;
  switch (span.style) {
    case "declared": {
      const target = span.target;
      if (target === undefined || !onToggle) {
        return <span className="type-name">{text}</span>;
      }
      const opened = open === index;
      return (
        <button
          type="button"
          className={`type-link${opened ? " open" : ""}`}
          aria-expanded={opened}
          onClick={(event) => {
            event.stopPropagation();
            onToggle(index, target);
          }}
        >
          {text}
        </button>
      );
    }
    case "external":
      return (
        <Tooltip content={`From ${span.module ?? span.note ?? "a package"}`} side="bottom" align="start">
          <span className="type-external" tabIndex={0}>
            {text}
          </span>
        </Tooltip>
      );
    case "unresolved":
      return (
        <Tooltip content={span.note ?? "Not resolved"} side="bottom" align="start">
          <span className="type-unresolved" tabIndex={0}>
            {text}
          </span>
        </Tooltip>
      );
    case "builtin":
      return <span className="type-builtin">{text}</span>;
  }
}

export function TypeText({ type, open = null, onToggle }: TypeTextProps) {
  return (
    <span className="type-text">
      {typeSegments(type).map((segment, position) =>
        segment.kind === "span" ? (
          <SpanText key={position} segment={segment} open={open} {...(onToggle ? { onToggle } : {})} />
        ) : (
          <span key={position} className={segment.kind === "word" ? "type-word" : "type-punct"}>
            {segment.text}
          </span>
        ),
      )}
    </span>
  );
}
