import { describe, expect, it } from 'vitest';
import { stepIndex } from './stepper';

describe('stepIndex', () => {
  it('moves one option in the arrow direction', () => {
    expect(stepIndex(5, 2, 1)).toBe(3);
    expect(stepIndex(5, 2, -1)).toBe(1);
  });

  it('stops at both ends instead of wrapping', () => {
    expect(stepIndex(5, 4, 1)).toBeNull();
    expect(stepIndex(5, 0, -1)).toBeNull();
  });

  it('enters an unknown value from the matching end', () => {
    expect(stepIndex(5, -1, 1)).toBe(0);
    expect(stepIndex(5, -1, -1)).toBe(4);
  });

  it('does nothing without options', () => {
    expect(stepIndex(0, -1, 1)).toBeNull();
  });
});
