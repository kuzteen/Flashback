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
