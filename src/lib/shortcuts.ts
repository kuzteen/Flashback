// Tabla única de atajos del editor: la lee el manejador de teclado y la pinta el menú de
// herramientas, así lo que el menú anuncia es siempre lo que el teclado hace.

export type ActionId =
  | 'play'
  | 'prevFrame'
  | 'nextFrame'
  | 'cut'
  | 'remove'
  | 'toggleDisable'
  | 'undo'
  | 'redo'
  | 'fullscreen'
  | 'reset';

// key es event.key normalizado: letras en minúscula, teclas con nombre tal cual.
export type KeyCombo = { key: string; ctrl?: boolean; shift?: boolean };

export type Shortcut = { action: ActionId; combos: KeyCombo[]; label: string };

export const SHORTCUTS: Shortcut[] = [
  { action: 'play', combos: [{ key: ' ' }, { key: 'k' }], label: 'ed.act.play' },
  { action: 'prevFrame', combos: [{ key: 'ArrowLeft' }], label: 'ed.act.prevFrame' },
  { action: 'nextFrame', combos: [{ key: 'ArrowRight' }], label: 'ed.act.nextFrame' },
  { action: 'cut', combos: [{ key: 'c' }], label: 'ed.act.cut' },
  { action: 'remove', combos: [{ key: 'Delete' }, { key: 'Backspace' }], label: 'ed.act.remove' },
  { action: 'toggleDisable', combos: [{ key: 'd' }], label: 'ed.act.toggleDisable' },
  { action: 'undo', combos: [{ key: 'z', ctrl: true }], label: 'ed.act.undo' },
  { action: 'redo', combos: [{ key: 'z', ctrl: true, shift: true }, { key: 'y', ctrl: true }], label: 'ed.act.redo' },
  { action: 'fullscreen', combos: [{ key: 'f' }], label: 'ed.act.fullscreen' },
  // Sin atajo a propósito: es destructivo y raro, se llega solo desde el menú.
  { action: 'reset', combos: [], label: 'ed.act.reset' },
];

function normalizeKey(key: string): string {
  return key.length === 1 ? key.toLowerCase() : key;
}

// Los modificadores tienen que coincidir exactos: Ctrl+C no es cortar. Alt queda libre para
// desactivar los imanes al arrastrar, y Meta no tiene atajos en Windows.
export function matchShortcut(e: {
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}): ActionId | null {
  if (e.altKey || e.metaKey) return null;
  const key = normalizeKey(e.key);
  for (const s of SHORTCUTS) {
    for (const c of s.combos) {
      if (c.key === key && !!c.ctrl === e.ctrlKey && !!c.shift === e.shiftKey) return s.action;
    }
  }
  return null;
}

const NAMED: Record<string, string> = {
  ' ': 'Space',
  Delete: 'Del',
  Backspace: 'Backspace',
  ArrowLeft: '←',
  ArrowRight: '→',
};

export function comboTokens(c: KeyCombo): string[] {
  const out: string[] = [];
  if (c.ctrl) out.push('Ctrl');
  if (c.shift) out.push('Shift');
  out.push(NAMED[c.key] ?? c.key.toUpperCase());
  return out;
}
