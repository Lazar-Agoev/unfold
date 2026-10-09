/** A section's caps label in a 28px row (components §4.2), with an optional trailing control or note. */

import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "./internal/dom";
import "./SectionLabel.css";

export interface SectionLabelProps extends HTMLAttributes<HTMLDivElement> {
  readonly children: ReactNode;
  /** An icon button (Pages' +) or a quiet note ("follow order"). */
  readonly trailing?: ReactNode;
  /** The heading level of the label. Default h2. */
  readonly as?: "h2" | "h3";
}

export function SectionLabel({ children, trailing, as: Heading = "h2", className, ...rest }: SectionLabelProps) {
  return (
    <div {...rest} className={cx("section-label", className)}>
      <Heading className="section-label-text">{children}</Heading>
      {trailing === undefined ? null : <div className="section-label-trailing">{trailing}</div>}
    </div>
  );
}
