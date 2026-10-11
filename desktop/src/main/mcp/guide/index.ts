/**
 * The guide (design §7.3): what agents read through `get_guide`, by topic. Each topic is a Markdown
 * file beside this one, bundled as text. A topic without a file isn't written yet, and `get_guide`
 * says so rather than make something up.
 */

import start from "./start.md?raw";

/** Every topic, in the order the guide lists them. */
export const GUIDE_TOPICS = ["start", "diagrams", "connections", "data-models", "graph", "writing", "setup", "repairs"] as const;

export type GuideTopic = (typeof GUIDE_TOPICS)[number];

const WRITTEN: Partial<Record<GuideTopic, string>> = { start };

/** A topic's text, or null when it isn't written yet. */
export function guideText(topic: GuideTopic): string | null {
  return WRITTEN[topic]?.trim() ?? null;
}
