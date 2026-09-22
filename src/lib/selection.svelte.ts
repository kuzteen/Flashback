import { SvelteSet } from 'svelte/reactivity';
import { clipOrder } from '$lib/editor-state.svelte';
import type { Clip } from '$lib/clips';

// La selección va por id de clip, nunca por índice: la rejilla virtualizada monta y desmonta
// tarjetas, y el orden cambia al filtrar o reordenar.
export const selected = new SvelteSet<string>();

// Ancla del último clip marcado, para que shift+click seleccione el rango intermedio.
let anchor: string | null = null;

export function isSelected(id: string): boolean {
  return selected.has(id);
}

export function clearSelection() {
  selected.clear();
  anchor = null;
}

export function pick(id: string, extend = false) {
  if (extend && anchor && anchor !== id) {
    const ids = clipOrder.list.map((c) => c.id);
    const from = ids.indexOf(anchor);
    const to = ids.indexOf(id);
    if (from !== -1 && to !== -1) {
      for (const x of ids.slice(Math.min(from, to), Math.max(from, to) + 1)) selected.add(x);
      anchor = id;
      return;
    }
  }
  if (selected.has(id)) {
    selected.delete(id);
    if (anchor === id) anchor = null;
  } else {
    selected.add(id);
    anchor = id;
  }
}

export function selectAll() {
  for (const c of clipOrder.list) selected.add(c.id);
}

// Un clip borrado (desde el menú de su tarjeta o desde el Explorador) dejaría su id aquí dentro,
// y la barra seguiría contando algo que ya no existe.
export function pruneSelection(clips: Clip[]) {
  if (selected.size === 0) return;
  const alive = new Set(clips.map((c) => c.id));
  for (const id of selected) if (!alive.has(id)) selected.delete(id);
  if (anchor && !alive.has(anchor)) anchor = null;
}
