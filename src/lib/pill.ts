import { tick } from 'svelte';
import type { Action } from 'svelte/action';
import { pillTiming, type Span } from './pill-math';

export type PillOpts = { key: unknown; axis?: 'x' | 'y'; selector?: string };

// Posición dentro del grupo por offsets y no por getBoundingClientRect: un diálogo que entra
// escalado daría medidas deformadas durante su animación.
function offsetIn(target: HTMLElement, root: HTMLElement): { x: number; y: number } | null {
  let x = 0;
  let y = 0;
  let el: HTMLElement | null = target;
  while (el && el !== root) {
    x += el.offsetLeft;
    y += el.offsetTop;
    el = el.offsetParent as HTMLElement | null;
    if (el && el !== root) {
      x += el.clientLeft;
      y += el.clientTop;
    }
  }
  return el === root ? { x, y } : null;
}

// El relleno en sí: un span dentro del grupo que se coloca sobre un botón y viaja al siguiente.
// Cada grupo le da su color y radio sobre `.slide-pill`.
function mountPill(node: HTMLElement, axis: 'x' | 'y') {
  let prev: Span | null = null;
  const el = document.createElement('span');
  el.className = `slide-pill ${axis}`;
  el.setAttribute('aria-hidden', 'true');
  node.prepend(el);
  node.dataset.pill = '';
  if (getComputedStyle(node).position === 'static') node.style.position = 'relative';

  function moveTo(target: HTMLElement | null, animate: boolean) {
    const at = target && offsetIn(target, node);
    if (!target || !at) {
      el.classList.remove('show');
      prev = null;
      return;
    }
    const w = target.offsetWidth;
    const h = target.offsetHeight;
    const span = axis === 'y' ? { start: at.y, end: at.y + h } : { start: at.x, end: at.x + w };
    const d = animate ? pillTiming(prev, span) : { start: 0, end: 0 };
    el.style.setProperty('--ds', `${d.start}s`);
    el.style.setProperty('--de', `${d.end}s`);
    el.style.inset = `${at.y}px ${node.clientWidth - at.x - w}px ${node.clientHeight - at.y - h}px ${at.x}px`;
    if (target.dataset.tone) el.dataset.tone = target.dataset.tone;
    else delete el.dataset.tone;
    el.classList.add('show');
    prev = span;
  }

  return { moveTo, destroy: () => el.remove() };
}

// Relleno del botón elegido de un grupo: viaja al nuevo en vez de saltar. El botón elegido deja de
// pintar su propio fondo.
export const pill: Action<HTMLElement, PillOpts> = (node, initial) => {
  let opts = initial;
  let tracked: HTMLElement | null = null;
  const p = mountPill(node, opts.axis ?? 'x');

  const ro = new ResizeObserver(() => place(false));
  ro.observe(node);

  function place(animate: boolean) {
    const target = node.querySelector<HTMLElement>(opts.selector ?? '.on');
    if (target !== tracked) {
      if (tracked) ro.unobserve(tracked);
      if (target) ro.observe(target);
      tracked = target;
    }
    p.moveTo(target, animate);
  }

  place(false);

  return {
    update(next) {
      const moved = next.key !== opts.key;
      opts = next;
      if (moved) void tick().then(() => place(true));
    },
    destroy() {
      ro.disconnect();
      p.destroy();
    }
  };
};

// Fondo de hover compartido por los botones de una barra o un menú: sigue al ratón de uno a otro.
// Sobre lo que no es un control (huecos, separadores, un contador) se queda donde está, para no
// apagarse y volver a encenderse; sobre otro control que no es del grupo (una casilla, un submenú
// con su propio fondo) se apaga. Un botón con data-tone tiñe el fondo.
// part: el relleno se coloca sobre esa pieza del botón en vez de sobre el botón entero.
export type HoverPillOpts = { selector?: string; axis?: 'x' | 'y'; part?: string };

export const hoverPill: Action<HTMLElement, HoverPillOpts | undefined> = (node, initial) => {
  let sel = initial?.selector ?? 'button';
  let part = initial?.part;
  let current: HTMLElement | null = null;
  const p = mountPill(node, initial?.axis ?? 'x');

  const over = (e: PointerEvent) => {
    const hit = e.target as HTMLElement;
    const target = hit.closest<HTMLElement>(sel);
    if (target && target.closest('[data-pill]') === node && !target.matches(':disabled')) {
      if (target === current) return;
      const box = (part && target.querySelector<HTMLElement>(part)) || target;
      p.moveTo(box, current !== null);
      current = target;
    } else {
      const other = hit.closest('button, a, input, [role="menu"]');
      if (other && other !== node && node.contains(other)) {
        p.moveTo(null, false);
        current = null;
      }
    }
  };
  const leave = () => {
    p.moveTo(null, false);
    current = null;
  };
  node.addEventListener('pointerover', over);
  node.addEventListener('pointerleave', leave);

  return {
    update(next) {
      sel = next?.selector ?? 'button';
      part = next?.part;
    },
    destroy() {
      node.removeEventListener('pointerover', over);
      node.removeEventListener('pointerleave', leave);
      p.destroy();
    }
  };
};
