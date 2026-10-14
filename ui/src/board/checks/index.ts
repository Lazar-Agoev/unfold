/**
 * The page checks (design §7.7): layout and content problems of a laid-out page, each with what fixes
 * it. The render window runs them on the page as the canvas drew it.
 */

export { pageChecks } from "./checks";
export { frameBox, frameTree } from "./frames";
export { boundsOf, type Box, type DrawnLine, type PageGeometry, type Point } from "./geometry";
