export {};
function withThis(this: Window, first: () => void, second: () => void) { second(); first(); }
function plain(first: () => void, second: () => void) { second(); first(); }
const arrow = (only: () => void) => only();
