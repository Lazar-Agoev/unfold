/**
 * The prompts (design §6.9): recipes the person starts from their agent, which Claude Code lists as
 * `/unfold:<name>`. Each becomes the person's message: read the matching guide topic, post a brief,
 * build, review with `check` and `get_screenshot`, and finish. Claude Code splits what the person
 * types after the command on whitespace, one word per argument, so each argument is one word (a
 * route's method and path are two), and every argument may be left out for what the person is
 * looking at.
 */

import { z } from "zod";
import { definePrompt } from "../prompt";

/** One optional word an argument takes. */
function word(description: string) {
  return z.string().trim().min(1).optional().describe(description);
}

/** The project a prompt names, or the one the person is viewing. */
function projectText(project: string | undefined): string {
  return project === undefined ? "the project I'm viewing in Unfold" : `the project “${project}”`;
}

/** The steps every prompt ends with: review, then finish. */
const REVIEW = "Review: arrange_page, then check and get_screenshot after each section, and fix what they find with targeted edits.";
const FINISH = "Finish: finish_work, then tell me in chat what you made and which parts go beyond what Unfold compiled, each with what it cites.";

export const explainEntrance = definePrompt({
  name: "explain-entrance",
  title: "Explain an entrance",
  description: "A page that explains one entrance (a route, worker, schedule, or command) end to end, for someone new to the code.",
  args: z.object({
    entrance: word("The entrance: a route's method (then its path), a queue, a command, or a handler's name; absent: what's selected in Unfold"),
    path: word("A route's path, after its method: POST /orders"),
  }),
  text: ({ entrance, path }) => {
    const named = [entrance, path].filter((part) => part !== undefined).join(" ");
    return [
      `Explain ${named.length > 0 ? `the entrance “${named}”` : "the entrance selected in Unfold"} end to end, on a page of its own, for someone new to this code.`,
      '- Read get_guide({topic: "diagrams"}) (its recipe "Explain a request") and get_guide({topic: "writing"}) first, unless you have this session. Read "connections" too if the path crosses a queue, an event, or another service.',
      "- Find it: get_current_view, then list_entrances with a filter from what I named. If several entrances match, ask me which one, naming each with its file:line.",
      "- Read before you place: read_code on the entrance (its handlers) and on the functions they reach that matter; get_relations where the path goes on.",
      "- Post a diagram brief in chat (question, reader, scope, shape, detail), then build: on the page I linked if I pasted one, else create_board traced from the entrance; the cards that matter (compact for context, expanded for the one or two functions the page is about, a call expanded in place where the reader needs it), combined steps (group_steps) and Flow labels on those, descriptions, and frames and connections where the path crosses services or queues.",
      `- ${REVIEW}`,
      `- ${FINISH}`,
    ].join("\n");
  },
});

export const simplifyFunction = definePrompt({
  name: "simplify-function",
  title: "Simplify a function",
  description: "Combined steps and Flow labels on one function's card, until it reads in a few steps expanded.",
  args: z.object({
    function: word("The function's name, such as createOrder or OrderService.place; absent: the card selected in Unfold"),
  }),
  text: ({ function: name }) =>
    [
      `Simplify ${name === undefined ? "the function whose card is selected in Unfold" : `the function “${name}”`}: combined steps and Flow labels until it reads in a few steps expanded.`,
      '- Read get_guide({topic: "diagrams"}) (its recipe "Simplify a function") and get_guide({topic: "writing"}) first, unless you have this session.',
      "- Find its card: on the page I linked if I pasted one, else get_current_view and the page I'm viewing. If no card there shows it, add_cards it there expanded; if no board is open, create_board for it.",
      '- read_code with view "flow": its steps, their lines, and their keys.',
      "- Post a short brief in chat: the 3 to 7 step groups (their lines and titles) and the steps left outside them. Then group_steps with collapsed: true (Combine), label_steps for every step (those inside the groups too), and set_view to show the card expanded.",
      "- Review: check, then get_screenshot of the card, and fix what they find.",
      "- Finish: finish_work, then tell me in chat how many steps its Flow reads in now, and what each step group holds.",
    ].join("\n"),
});

export const serviceMap = definePrompt({
  name: "service-map",
  title: "Service map",
  description: "A page with a frame per service, its entrances, and how the services talk through HTTP, queues, events, and stores.",
  args: z.object({
    project: word("The project, by name; absent: the project I'm viewing"),
  }),
  text: ({ project }) =>
    [
      `Map the services of ${projectText(project)} on one page: a frame per service with its entrances, and how the services talk.`,
      '- Read get_guide({topic: "diagrams"}) (its recipe "Service map") and get_guide({topic: "connections"}) first, unless you have this session.',
      "- Understand: list_entrances. search text for where services talk (queues, HTTP clients, events, stores), and read_code each hit before you draw anything.",
      "- Post a diagram brief in chat, then build: create_board (or the page I linked), the entrances compact, frame_cards with a box per application inside a zone, add_resources for the queues and stores (and an external card for a service in another repository), and connect producers to consumers, each anchored where the code shows it. Shared code goes in a box of its own.",
      `- ${REVIEW}`,
      `- ${FINISH} Name what you saw but couldn't cite.`,
    ].join("\n"),
});

export const dataModel = definePrompt({
  name: "data-model",
  title: "Data model",
  description: "A data model page: the tables (or types) and their relationships, and which code reads and writes each table.",
  args: z.object({ project: word("The project, by name; absent: the project I'm viewing") }),
  text: ({ project }) =>
    [
      `Draw the data model of ${projectText(project)}: its tables (or the types it passes around), the relationships between them, and which code reads and writes each table.`,
      '- Read get_guide({topic: "data-models"}) and get_guide({topic: "connections"}) first, unless you have this session.',
      '- Understand: search with kinds ["types"], read_code on each candidate, and get_relations for who uses it. Prefer tables backed by the code\'s types, so their columns are compiled. If the code keeps its data in memory, say so. If the schema lives in files Unfold doesn\'t compile (migrations, a Prisma schema), ask me before adding them as evidence files.',
      "- Post a diagram brief in chat, then build: add_resources for the tables (backed by their types, with key marks), connect relates only where the types don't link, add_cards for the code that reads and writes them, and connect reads and writes from the lines that do it.",
      `- ${REVIEW}`,
      `- ${FINISH} Say which columns are compiled and which you listed yourself.`,
    ].join("\n"),
});

export const tour = definePrompt({
  name: "tour",
  title: "Tour",
  description: "An onboarding board for a project: how it starts, then a page for each main path through it.",
  args: z.object({ project: word("The project, by name; absent: the project I'm viewing") }),
  text: ({ project }) =>
    [
      `Make an onboarding board for ${projectText(project)}, for someone new to it: a first page on how it starts, then a page for each main path through it.`,
      '- Read get_guide({topic: "diagrams"}) and get_guide({topic: "writing"}) first, unless you have this session; "connections" too when a path crosses a queue or another service.',
      "- Understand: get_project and list_entrances for its applications and entrances. Choose the 3 to 5 paths a newcomer meets first, and read their code.",
      "- Post a brief in chat for the board: its pages in order and the question each answers. Then build: create_board with those pages; the first page a map of its applications and their entrances, all compact; each other page explains one path, as the recipe \"Explain a request\" does, with its own short brief.",
      `- ${REVIEW}`,
      `- ${FINISH}`,
    ].join("\n"),
});

export const fixBoards = definePrompt({
  name: "fix-boards",
  title: "Fix boards",
  description: "Repairs for every board after a refactor: reconnect what moved, re-sign what agents wrote that still holds, and list what's yours to sign off.",
  args: z.object({ project: word("The project, by name; absent: the project I'm viewing") }),
  text: ({ project }) =>
    [
      `Fix the boards of ${projectText(project)} after a refactor: reconnect what moved, re-sign what's still true, and tell me what's mine to sign off.`,
      '- Read get_guide({topic: "repairs"}) first, unless you have this session.',
      "- Find the problems: list_boards, then check each board that needs review.",
      "- Post a short brief in chat: for each board, its problems and the repair you'll make. Then repair each: find the new key (search, get_relations), read_code the new code, and reconnect. Re-sign or accept only what an agent wrote, after reading the new code. Remove only what is gone for good.",
      "- Review: check each board again until only my sign-offs are left, and get_screenshot a page you took items off.",
      "- Finish: finish_work, then tell me in chat what you fixed on each board, and what's left for me to re-sign or decide, with what changed in the code.",
    ].join("\n"),
});
