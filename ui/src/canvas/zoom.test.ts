import { describe, expect, it } from "vitest";
import { zoomScale } from "./zoom";

/** A size in canvas px as it reads on screen at `zoom`. */
const onScreen = (size: number, zoom: number) => size * zoom;

describe("the zoom", () => {
  it("grows names on screen from 13px at 100% to 15px at 50%; further out they shrink with the card, closer they scale with it", () => {
    const name = (zoom: number) => onScreen(13 * zoomScale(zoom).nameGrow, zoom);
    expect(name(1)).toBeCloseTo(13);
    expect(name(0.75)).toBeCloseTo(14);
    expect(name(0.5)).toBeCloseTo(15);
    expect(name(0.25)).toBeCloseTo(7.5);
    expect(zoomScale(1.5).nameGrow).toBe(1);
  });

  it("grows box titles 13 → 14 → 16px and zone titles 20 → 26 → 34px on screen at 100, 50, and 25%, and on past", () => {
    const titles = (zoom: number) => {
      const scale = zoomScale(zoom);
      return [onScreen(scale.box.title, zoom), onScreen(scale.zone.title, zoom)].map((size) => Math.round(size * 100) / 100);
    };
    expect(titles(1)).toEqual([13, 20]);
    expect(titles(0.5)).toEqual([14, 26]);
    expect(titles(0.25)).toEqual([16, 34]);
    expect(titles(0.1)[0]).toBeGreaterThan(16);
    expect(zoomScale(1.5).box.title).toBe(13);
  });

  it("moves a frame's edge out with its title, so the title stays inside, and thickens it", () => {
    const edges = (zoom: number) => {
      const { box, zone } = zoomScale(zoom);
      return [onScreen(box.edge, zoom), onScreen(zone.edge, zoom)].map((width) => Math.round(width * 100) / 100);
    };
    expect(edges(1)).toEqual([1, 0]);
    expect(edges(0.5)).toEqual([1.5, 1.5]);
    expect(edges(0.25)).toEqual([2, 2]);
    // At 100% and closer the frame's padding holds the title: nothing moves.
    expect(zoomScale(1).box).toMatchObject({ titleTop: 12, titleLeft: 16, growTop: 0, growSide: 0 });
    expect(zoomScale(1.5).zone).toMatchObject({ titleTop: 18, titleLeft: 20, growTop: 0, growSide: 0 });
    // Further out the edge moves up by what the bigger title needs beyond the 44px the box leaves over its cards.
    const box = zoomScale(0.25).box;
    expect(box.growTop).toBeCloseTo(box.titleTop + box.title * (18 / 13) + 20 - 44);
    expect(onScreen(box.titleTop, 0.25)).toBe(6);
    expect(box.growSide).toBe(8);
    expect(zoomScale(0.5).zone.growTop).toBeGreaterThan(0);
  });

  it("fades chips and descriptions below 60%, and keeps the writing chip's size on screen", () => {
    expect(zoomScale(0.6).far).toBe(false);
    expect(zoomScale(0.59).far).toBe(true);
    expect(zoomScale(0.25).screen).toBe(4);
    expect(zoomScale(1.5).screen).toBeCloseTo(2 / 3);
    // A connection's card is never drawn smaller than it reads at 100%.
    expect(zoomScale(0.25).hover).toBe(4);
    expect(zoomScale(1.5).hover).toBe(1);
  });
});
