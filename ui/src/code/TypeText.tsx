/**
 * A written type (`TypeText`, docs/reader.md): its text with one span per type name
 * in it. A `declared` name reads as a name; an `external` one has a dotted underline whose tooltip
 * names its package; a `builtin` one reads like punctuation; an `unresolved` one has a dashed
 * underline whose tooltip says why. Between names, words read as text and punctuation (`[]`, `<>`,
 * `|`) in the contour gray.
 */

import { Tooltip } from "../components";
import type { TypeSpan, TypeText as TypeTextModel } from "../ipc/generated";
import "./types.css";

type TypeSegment =
  | { readonly kind: "word" | "punct"; readonly text: string }
  | { readonly kind: "span"; readonly text: string; readonly span: TypeSpan };

const WORD = /[\p{L}\p{N}_$.]+|[^\p{L}\p{N}_$.]+/gu;
const WORD_START = /^[\p{L}\p{N}_$]/u;

function plain(text: string): TypeSegment[] {
  return [...text.matchAll(WORD)].map((match) => ({ kind: WORD_START.test(match[0]) ? ("word" as const) : ("punct" as const), text: match[0] }));
}

/** The type's text cut at its spans (UTF-16 offsets, as JavaScript strings index). */
export function typeSegments(type: TypeTextModel): TypeSegment[] {
  const segments: TypeSegment[] = [];
  let at = 0;
  const spans = [...type.spans].sort((a, b) => a.start - b.start);
  for (const span of spans) {
    if (span.start < at || span.end > type.text.length || span.end <= span.start) {
      continue;
    }
    segments.push(...plain(type.text.slice(at, span.start)));
    segments.push({ kind: "span", text: type.text.slice(span.start, span.end), span });
    at = span.end;
  }
  segments.push(...plain(type.text.slice(at)));
  return segments;
}

function SpanText({ segment }: { readonly segment: Extract<TypeSegment, { kind: "span" }> }) {
  const { span, text } = segment;
  switch (span.style) {
    case "declared":
      return <span className="type-name">{text}</span>;
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

export function TypeText({ type }: { readonly type: TypeTextModel }) {
  return (
    <span className="type-text">
      {typeSegments(type).map((segment, position) =>
        segment.kind === "span" ? (
          <SpanText key={position} segment={segment} />
        ) : (
          <span key={position} className={segment.kind === "word" ? "type-word" : "type-punct"}>
            {segment.text}
          </span>
        ),
      )}
    </span>
  );
}
