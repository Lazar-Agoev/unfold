/**
 * What jsdom lacks for the components: layout sizes (so virtualized lists render rows),
 * ResizeObserver, and scrollIntoView. Import from a jsdom test before rendering.
 */

const VIEWPORT = { width: 400, height: 220 };

for (const [property, size] of [
  ["offsetWidth", VIEWPORT.width],
  ["offsetHeight", VIEWPORT.height],
  ["clientWidth", VIEWPORT.width],
  ["clientHeight", VIEWPORT.height],
] as const) {
  Object.defineProperty(HTMLElement.prototype, property, { configurable: true, get: () => size });
}

class NoopResizeObserver {
  observe(): void {
    // jsdom has no layout, so sizes never change.
  }
  unobserve(): void {
    // Nothing observed.
  }
  disconnect(): void {
    // Nothing observed.
  }
}

if (!("ResizeObserver" in globalThis)) {
  Object.defineProperty(globalThis, "ResizeObserver", { configurable: true, value: NoopResizeObserver });
}

Element.prototype.scrollIntoView = function scrollIntoView() {
  // jsdom doesn't scroll.
};
