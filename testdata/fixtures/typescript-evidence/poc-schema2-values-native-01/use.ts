import * as source from "./source.js";
function callback() {} const constant = callback; let mutable = callback;
const refs = [callback, constant, mutable, source["action"], source[key], absent, (callback as unknown)!];
function scoped(callback: () => void) { const refs = [callback]; }