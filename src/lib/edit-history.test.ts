import { describe, expect, it } from 'vitest';
import { EditHistory } from './edit-history';

describe('EditHistory', () => {
  it('deshace y rehace volviendo al estado exacto', () => {
    const h = new EditHistory<number>();
    h.record(1, 2);
    h.record(2, 3);
    expect(h.undo(3)).toBe(2);
    expect(h.undo(2)).toBe(1);
    expect(h.undo(1)).toBeNull();
    expect(h.redo(1)).toBe(2);
    expect(h.redo(2)).toBe(3);
    expect(h.redo(3)).toBeNull();
  });

  it('un gesto que no cambia nada no se registra', () => {
    const h = new EditHistory<number>();
    h.record(5, 5);
    expect(h.canUndo).toBe(false);
  });

  it('un gesto largo registrado al soltar es un solo paso', () => {
    const h = new EditHistory<number>();
    const before = 10;
    // durante el arrastre el estado pasa por 11, 12, 13 sin registrarse
    h.record(before, 13);
    expect(h.undo(13)).toBe(10);
    expect(h.canUndo).toBe(false);
  });

  it('un paso nuevo vacía lo que se podía rehacer', () => {
    const h = new EditHistory<number>();
    h.record(1, 2);
    h.undo(2);
    h.record(1, 7);
    expect(h.canRedo).toBe(false);
  });

  it('respeta el límite descartando lo más antiguo', () => {
    const h = new EditHistory<number>(3);
    for (let i = 0; i < 5; i++) h.record(i, i + 1);
    expect(h.undo(5)).toBe(4);
    expect(h.undo(4)).toBe(3);
    expect(h.undo(3)).toBe(2);
    expect(h.undo(2)).toBeNull();
  });

  it('compara objetos por contenido por defecto', () => {
    const h = new EditHistory<{ a: number }>();
    h.record({ a: 1 }, { a: 1 });
    expect(h.canUndo).toBe(false);
  });

  it('clear lo vacía todo', () => {
    const h = new EditHistory<number>();
    h.record(1, 2);
    h.undo(2);
    h.clear();
    expect(h.canUndo).toBe(false);
    expect(h.canRedo).toBe(false);
  });
});
