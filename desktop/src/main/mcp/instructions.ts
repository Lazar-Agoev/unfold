/**
 * The server's instructions (design §7.2): what an agent reads when its session starts, before any
 * tool. Claude Code cuts them at 2,048 characters, and Codex wants the first 512 to stand on their
 * own, so the first paragraph says what Unfold is and ends with the call to read the guide.
 */

export const INSTRUCTIONS = `Unfold compiles TypeScript and Python repos into a graph of entrances, functions,
types, and calls, shown on boards: pages of cards in Diagram, Flow, or Code view.
These tools set up projects and build pages that explain code: group steps, frame
cards into units and services, add tables and queues, and draw typed connections.
You place compiled code; you never write code or change what the compiler proved.
Call get_guide({topic: "start"}) before any other Unfold tool.

Rules:
- Before a job, read the guide topic it needs (start lists them): diagrams before
  building a page, connections before connect, repairs before repair.
- Work on the page the person is viewing (get_current_view) or on a link they
  paste. Once you start, pass board and page ids; the person may switch pages.
- Everything you add must stand on compiled code: place cards by key, and give
  every table, queue, and connection anchors. If you can't cite it, say so in chat.
- If a project isn't compiled or current, compile_project before reading it.
- Before building a new page, post a short diagram brief in chat.
- Build in steps the person can watch: one frame, or one kind of change, per call.
- Review: call check and get_screenshot after each section, and fix what they find
  before going on.
- Answer questions in chat. Change boards, settings, or projects only when asked.
- People's words are theirs: replace or remove them only when asked, and re-sign
  or accept only what an agent wrote.
- Never show ids or keys to the person; use names and file:line.
- When you're done, call finish_work with a one-line summary.`;
