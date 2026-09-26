import { describe, expect, it } from 'vitest';
import { placeTip, splitSwap, tipDirection } from './tip-math';

const VIEW = { w: 1920, h: 1032 };

describe('placeTip', () => {
  it('sits to the right of the anchor, vertically centred', () => {
    const p = placeTip({ left: 9, top: 74, width: 52, height: 52 }, { w: 80, h: 36 }, 'right', VIEW);
    expect(p).toEqual({ x: 61 + 10, y: 100 - 18, caret: 18 });
  });

  it('sits below the anchor, horizontally centred', () => {
    const p = placeTip({ left: 500, top: 80, width: 40, height: 30 }, { w: 100, h: 34 }, 'bottom', VIEW);
    expect(p).toEqual({ x: 470, y: 110 + 10, caret: 50 });
  });

  it('sits above the anchor for top tips', () => {
    const p = placeTip({ left: 500, top: 300, width: 40, height: 30 }, { w: 100, h: 34 }, 'top', VIEW);
    expect(p).toEqual({ x: 470, y: 300 - 10 - 34, caret: 50 });
  });

  it('keeps a bottom tip inside the window and moves the caret to stay on the anchor', () => {
    const p = placeTip({ left: 1850, top: 80, width: 30, height: 30 }, { w: 120, h: 34 }, 'bottom', VIEW);
    expect(p.x).toBe(1920 - 8 - 120);
    expect(p.caret).toBe(1865 - p.x);
  });

  it('never pushes the caret past the rounded corners', () => {
    const p = placeTip({ left: 1908, top: 80, width: 12, height: 30 }, { w: 120, h: 34 }, 'bottom', VIEW);
    expect(p.caret).toBe(120 - 14);
  });
});

describe('tipDirection', () => {
  it('is positive when moving down or right and negative the other way', () => {
    const a = { left: 9, top: 74, width: 52, height: 52 };
    const b = { left: 9, top: 130, width: 52, height: 52 };
    expect(tipDirection(a, b, 'right')).toBe(1);
    expect(tipDirection(b, a, 'right')).toBe(-1);
    expect(tipDirection({ ...a, left: 100 }, { ...a, left: 140 }, 'bottom')).toBe(1);
  });
});

describe('splitSwap', () => {
  it('keeps the shared words still and swaps only the rest', () => {
    expect(splitSwap('This clip is already under 50 MB', 'This clip is already under 100 MB')).toEqual({
      keep: 'This clip is already under ',
      swap: '100 MB'
    });
  });

  it('swaps the whole label when the start differs', () => {
    expect(splitSwap('All clips', 'Playlists')).toEqual({ keep: '', swap: 'Playlists' });
  });

  it('never splits a word in half', () => {
    expect(splitSwap('Card view', 'Compact view')).toEqual({ keep: '', swap: 'Compact view' });
  });

  it('swaps everything on the first label', () => {
    expect(splitSwap('', 'Settings')).toEqual({ keep: '', swap: 'Settings' });
  });
});
