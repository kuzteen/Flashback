import { describe, expect, it } from 'vitest';
import { flipTo } from './flip';

const VH = 800;

describe('flipTo', () => {
  it('leaves a menu that fits below where it is', () => {
    expect(flipTo({ top: 404, bottom: 604 }, { top: 366, bottom: 400 }, VH)).toBeNull();
  });

  it('opens upwards when it overflows the bottom and there is more room above', () => {
    expect(flipTo({ top: 734, bottom: 934 }, { top: 696, bottom: 730 }, VH)).toEqual({ side: 'up', gap: 4 });
  });

  it('stays below when the room above is even smaller', () => {
    expect(flipTo({ top: 124, bottom: 924 }, { top: 90, bottom: 120 }, VH)).toBeNull();
  });

  it('opens downwards when a menu above the button overflows the top', () => {
    expect(flipTo({ top: -100, bottom: 92 }, { top: 100, bottom: 130 }, VH)).toEqual({ side: 'down', gap: 8 });
  });

  it('keeps a menu above the button when it fits', () => {
    expect(flipTo({ top: 400, bottom: 592 }, { top: 600, bottom: 630 }, VH)).toBeNull();
  });

  it('ignores a menu that overlaps its button', () => {
    expect(flipTo({ top: 110, bottom: 900 }, { top: 100, bottom: 130 }, VH)).toBeNull();
  });
});
