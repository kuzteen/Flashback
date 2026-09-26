import { describe, expect, it } from 'vitest';
import { LEAD, TRAIL, pillTiming } from './pill-math';

describe('pillTiming', () => {
  it('moves the far edge first when going forward', () => {
    expect(pillTiming({ start: 0, end: 84 }, { start: 90, end: 174 })).toEqual({ start: TRAIL, end: LEAD });
  });

  it('moves the near edge first when going back', () => {
    expect(pillTiming({ start: 90, end: 174 }, { start: 0, end: 84 })).toEqual({ start: LEAD, end: TRAIL });
  });

  it('does not animate the first placement', () => {
    expect(pillTiming(null, { start: 0, end: 84 })).toEqual({ start: 0, end: 0 });
  });

  it('does not animate when nothing moved', () => {
    expect(pillTiming({ start: 0, end: 84 }, { start: 0, end: 84 })).toEqual({ start: 0, end: 0 });
  });
});
