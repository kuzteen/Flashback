import { describe, expect, it } from 'vitest';
import { takenBy } from './hotkey-conflict';

const keys = { saveReplay: 'Alt+F8', record: 'Alt+F9', open: 'Alt+F10' };

describe('takenBy', () => {
  it('names the action that already uses the combo', () => {
    expect(takenBy(keys, 'saveReplay', 'Alt+F9')).toBe('record');
  });

  it('ignores the action being rebound', () => {
    expect(takenBy(keys, 'saveReplay', 'Alt+F8')).toBeNull();
  });

  it('matches regardless of letter case', () => {
    expect(takenBy({ ...keys, open: 'Control+K' }, 'record', 'Control+k')).toBe('open');
  });

  it('returns null for a free combo', () => {
    expect(takenBy(keys, 'open', 'Alt+F11')).toBeNull();
  });
});
