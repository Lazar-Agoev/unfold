/**
 * The guide (design §7.3): what agents read through `get_guide`, by topic. Each topic is a Markdown
 * file beside this one, bundled as text. `start` is the one every agent reads first; the others are
 * read before the job they teach (the start topic lists when).
 */

import connections from "./connections.md?raw";
import dataModels from "./data-models.md?raw";
import diagrams from "./diagrams.md?raw";
import graph from "./graph.md?raw";
import repairs from "./repairs.md?raw";
import setup from "./setup.md?raw";
import start from "./start.md?raw";
import writing from "./writing.md?raw";

/** Every topic, in the order the guide lists them. */
export const GUIDE_TOPICS = ["start", "diagrams", "connections", "data-models", "graph", "writing", "setup", "repairs"] as const;

export type GuideTopic = (typeof GUIDE_TOPICS)[number];

const TOPICS: Readonly<Record<GuideTopic, string>> = {
  start,
  diagrams,
  connections,
  "data-models": dataModels,
  graph,
  writing,
  setup,
  repairs,
};

/** A topic's text. */
export function guideText(topic: GuideTopic): string {
  return TOPICS[topic].trim();
}
