import { describe, expect, it } from 'vitest';
import { SIZES, springFor, step, type Spring } from './switch-physics';

function run(s: Spring, to: number, bounce: number, seconds = 1.5) {
  const spring = springFor(0.3, bounce);
  let peak = s.x;
  let moving = true;
  for (let t = 0; t < seconds && moving; t += 1 / 60) {
    moving = step(s, to, spring, 1 / 60);
    peak = Math.max(peak, s.x);
  }
  return { peak, moving };
}

describe('switch spring', () => {
  it('settles exactly on the target', () => {
    const s = { x: 0, v: 0 };
    const { moving } = run(s, 1, 0.25);
    expect(moving).toBe(false);
    expect(s).toEqual({ x: 1, v: 0 });
  });

  it('overshoots a little with bounce, so the knob squashes against the wall', () => {
    const { peak } = run({ x: 0, v: 0 }, 1, 0.25);
    expect(peak).toBeGreaterThan(1.01);
    expect(peak).toBeLessThan(1.2);
  });

  it('never overshoots without bounce', () => {
    const { peak } = run({ x: 0, v: 0 }, 1, 0);
    expect(peak).toBeLessThanOrEqual(1);
  });

  it('keeps the knob inside the track for every size', () => {
    for (const d of Object.values(SIZES)) {
      expect(d.inner).toBe(d.w - 2 - 4);
      expect(d.stretched).toBeLessThan(d.inner);
      expect(d.knob).toBe(d.h - 2 - 4);
    }
  });
});
