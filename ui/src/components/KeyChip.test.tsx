// @vitest-environment jsdom
import "../test/dom";
import { render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { KeyChip } from "./KeyChip";

function onPlatform(platform: string) {
  Object.defineProperty(navigator, "platform", { configurable: true, value: platform });
}

afterEach(() => {
  onPlatform("");
});

describe("KeyChip", () => {
  it("draws a filled chip on accent and an outlined one in hints", () => {
    const { container } = render(
      <>
        <KeyChip variant="filled" glyph="A" />
        <KeyChip variant="outlined" glyph="esc" />
      </>,
    );
    const [filled, outlined] = container.querySelectorAll("kbd");
    expect(filled?.className).toBe("key-chip key-chip-filled");
    expect(outlined?.textContent).toBe("esc");
  });

  it("splits a shortcut into one keycap per key, in the platform's style", () => {
    onPlatform("MacIntel");
    const mac = render(<KeyChip variant="keycap" shortcut="Shift+Mod+E" selected />).container;
    expect([...mac.querySelectorAll(".key-chip")].map((cap) => cap.textContent)).toEqual(["⇧", "⌘", "E"]);
    expect(mac.querySelector(".key-chip")?.classList.contains("key-chip-selected")).toBe(true);
    expect(mac.querySelector("kbd")?.getAttribute("aria-label")).toBe("Shift Command E");

    onPlatform("Win32");
    const windows = render(<KeyChip variant="keycap" shortcut="Shift+Mod+E" />).container;
    expect([...windows.querySelectorAll(".key-chip")].map((cap) => cap.textContent)).toEqual(["Ctrl", "Shift", "E"]);
  });

  it("sets Add's > keycap in the code face", () => {
    const { container } = render(<KeyChip variant="keycap" glyph=">" />);
    expect(container.querySelector("kbd")?.classList.contains("key-chip-code")).toBe(true);
  });
});
