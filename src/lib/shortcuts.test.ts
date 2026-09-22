import { describe, expect, it } from 'vitest';
import { SHORTCUTS, comboTokens, matchShortcut } from './shortcuts';

const ev = (key: string, mods: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }> = {}) => ({
  key,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

describe('matchShortcut', () => {
  it('reconoce las teclas simples sin importar mayúsculas', () => {
    expect(matchShortcut(ev('c'))).toBe('cut');
    expect(matchShortcut(ev('C'))).toBe('cut');
    expect(matchShortcut(ev(' '))).toBe('play');
    expect(matchShortcut(ev('k'))).toBe('play');
    expect(matchShortcut(ev('d'))).toBe('toggleDisable');
    expect(matchShortcut(ev('Delete'))).toBe('remove');
    expect(matchShortcut(ev('Backspace'))).toBe('remove');
    expect(matchShortcut(ev('ArrowLeft'))).toBe('prevFrame');
    expect(matchShortcut(ev('ArrowRight'))).toBe('nextFrame');
    expect(matchShortcut(ev('f'))).toBe('fullscreen');
  });

  it('distingue deshacer de rehacer por los modificadores', () => {
    expect(matchShortcut(ev('z', { ctrlKey: true }))).toBe('undo');
    expect(matchShortcut(ev('Z', { ctrlKey: true, shiftKey: true }))).toBe('redo');
    expect(matchShortcut(ev('y', { ctrlKey: true }))).toBe('redo');
  });

  it('un modificador de más no dispara la tecla simple', () => {
    expect(matchShortcut(ev('c', { ctrlKey: true }))).toBeNull();
    expect(matchShortcut(ev('c', { shiftKey: true }))).toBeNull();
  });

  it('Alt y Meta nunca son atajos del editor', () => {
    expect(matchShortcut(ev('c', { altKey: true }))).toBeNull();
    expect(matchShortcut(ev('z', { ctrlKey: true, metaKey: true }))).toBeNull();
  });

  it('una tecla sin atajo no hace nada', () => {
    expect(matchShortcut(ev('q'))).toBeNull();
  });
});

describe('SHORTCUTS', () => {
  it('cada acción aparece una sola vez y tiene etiqueta', () => {
    const actions = SHORTCUTS.map((s) => s.action);
    expect(new Set(actions).size).toBe(actions.length);
    for (const s of SHORTCUTS) expect(s.label).toMatch(/^ed\.act\./);
  });

  it('restablecer no tiene atajo', () => {
    expect(SHORTCUTS.find((s) => s.action === 'reset')?.combos).toEqual([]);
  });
});

describe('comboTokens', () => {
  it('da los tokens en orden de lectura', () => {
    expect(comboTokens({ key: 'z', ctrl: true, shift: true })).toEqual(['Ctrl', 'Shift', 'Z']);
    expect(comboTokens({ key: ' ' })).toEqual(['Space']);
    expect(comboTokens({ key: 'Delete' })).toEqual(['Del']);
    expect(comboTokens({ key: 'ArrowLeft' })).toEqual(['←']);
  });
});
