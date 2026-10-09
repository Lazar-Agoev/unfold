/**
 * The line/token/link renderer shared by Code cards and source tabs: one range of shown text,
 * colored by the model's token classes (v7 syntax palette) with call links at their written ranges.
 * Callers decide what a link is (`renderLink`); without it, linked text renders plain.
 */

import { Fragment, memo, type ReactNode } from "react";
import { groupByLink, segmentRange, type LinkRange, type SpanRange } from "./segments";
import "./code.css";

export interface CodeTextProps {
  /** The shown text the offsets index into. */
  readonly text: string;
  /** The range to render, as offsets into `text`. */
  readonly start: number;
  readonly end: number;
  readonly spans: readonly SpanRange[];
  readonly links: readonly LinkRange[];
  /** Renders one link (by its index in the model's `links`) around its colored runs. */
  readonly renderLink?: (link: number, children: ReactNode) => ReactNode;
}

/** The CSS class of a token class: `tok tok-keyword`. */
export function tokenClass(token: string | null): string | undefined {
  return token === null ? undefined : `tok tok-${token}`;
}

export const CodeText = memo(function CodeText({ text, start, end, spans, links, renderLink }: CodeTextProps) {
  const groups = groupByLink(segmentRange(start, end, spans, links));
  return (
    <>
      {groups.map((group) => {
        const first = group.segments[0];
        const key = first ? first.start : 0;
        const runs = group.segments.map((segment) => (
          <span key={segment.start} className={tokenClass(segment.token)}>
            {text.slice(segment.start, segment.end)}
          </span>
        ));
        if (group.link === null || !renderLink) {
          return <Fragment key={`r${key}`}>{runs}</Fragment>;
        }
        return <Fragment key={`l${key}`}>{renderLink(group.link, runs)}</Fragment>;
      })}
    </>
  );
});
