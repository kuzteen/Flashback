// Historial por copias completas en vez de operaciones inversas: el estado del editor es
// pequeño, copiarlo es barato, y deshacer devuelve exactamente lo que había sin depender de que
// cada operación sepa revertirse.
export class EditHistory<T> {
  private past: T[] = [];
  private future: T[] = [];

  constructor(
    private readonly limit = 100,
    private readonly equal: (a: T, b: T) => boolean = (a, b) => JSON.stringify(a) === JSON.stringify(b),
  ) {}

  // Un gesto continuo (arrastrar un borde, deslizar un volumen) se registra una sola vez al
  // soltar, con el estado de antes de empezar: si no, deshacer iría milímetro a milímetro.
  record(before: T, after: T): void {
    if (this.equal(before, after)) return;
    this.past.push(before);
    if (this.past.length > this.limit) this.past.shift();
    this.future = [];
  }

  undo(current: T): T | null {
    const prev = this.past.pop();
    if (prev === undefined) return null;
    this.future.push(current);
    return prev;
  }

  redo(current: T): T | null {
    const next = this.future.pop();
    if (next === undefined) return null;
    this.past.push(current);
    return next;
  }

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  clear(): void {
    this.past = [];
    this.future = [];
  }
}
