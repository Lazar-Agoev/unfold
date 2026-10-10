/** A small outlined label with an optional icon ("File exists" in Project settings). */

import type { HTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "../icons";
import { cx } from "./internal/dom";
import "./Badge.css";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  readonly icon?: IconName;
  readonly children: ReactNode;
}

export function Badge({ icon, children, className, ...rest }: BadgeProps) {
  return (
    <span {...rest} className={cx("badge", className)}>
      {icon === undefined ? null : <Icon name={icon} size={12} />}
      {children}
    </span>
  );
}
