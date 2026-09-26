import type { Action } from 'svelte/action';
import { splitSwap, tipDirection, type Rect, type TipSide } from './tip-math';

// Lo justo para que cruzar la barra de camino a otra parte no encienda el globo.
const OPEN_INTENT = 80;
// Margen antes de cerrar: pasar al botón de al lado cruzando el hueco entre ambos no lo cierra,
// así que el globo viaja al nuevo en vez de apagarse y volver a encenderse.
const CLOSE_GRACE = 180;

const rectOf = (el: HTMLElement): Rect => {
  const r = el.getBoundingClientRect();
  return { left: r.left, top: r.top, width: r.width, height: r.height };
};

// Un único globo por grupo de botones vecinos.
export class TipGroup {
  open = $state(false);
  label = $state('');
  // Parte inicial que comparte con el texto anterior: se queda quieta y solo se anima el resto.
  keep = $state('');
  anchor = $state<Rect | null>(null);
  dir = $state<1 | -1>(1);
  // Cambia con cada texto nuevo; el componente lo usa de clave para deslizar el anterior fuera.
  seq = $state(0);
  // true cuando el globo aparece desde cerrado: se coloca sin viajar desde su sitio anterior.
  fresh = $state(true);

  readonly side: TipSide;
  private current: HTMLElement | null = null;
  private openTimer: ReturnType<typeof setTimeout> | undefined;
  private closeTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(side: TipSide) {
    this.side = side;
  }

  private show(el: HTMLElement, label: string, delay: boolean) {
    if (!label) return;
    clearTimeout(this.closeTimer);
    if (this.current === el) return;
    clearTimeout(this.openTimer);
    if (this.current && this.anchor) {
      const next = rectOf(el);
      this.dir = tipDirection(this.anchor, next, this.side);
      this.fresh = false;
      this.set(el, label, next);
      return;
    }
    const go = () => {
      this.fresh = true;
      this.set(el, label, rectOf(el));
      this.open = true;
      window.addEventListener('scroll', this.hideNow, true);
    };
    if (delay) this.openTimer = setTimeout(go, OPEN_INTENT);
    else go();
  }

  private set(el: HTMLElement, label: string, rect: Rect) {
    this.current = el;
    this.anchor = rect;
    this.keep = this.fresh ? '' : splitSwap(this.label, label).keep;
    this.label = label;
    this.seq++;
  }

  private hide() {
    clearTimeout(this.openTimer);
    clearTimeout(this.closeTimer);
    this.closeTimer = setTimeout(this.hideNow, CLOSE_GRACE);
  }

  hideNow = () => {
    clearTimeout(this.openTimer);
    clearTimeout(this.closeTimer);
    this.current = null;
    this.open = false;
    window.removeEventListener('scroll', this.hideNow, true);
  };

  trigger: Action<HTMLElement, string> = (node, initial) => {
    let label = initial ?? '';
    const enter = () => this.show(node, label, true);
    const leave = () => this.hide();
    const focus = () => node.matches(':focus-visible') && this.show(node, label, false);
    node.addEventListener('pointerenter', enter);
    node.addEventListener('pointerleave', leave);
    node.addEventListener('focusin', focus);
    node.addEventListener('focusout', leave);
    return {
      update: (next: string) => {
        label = next ?? '';
        if (this.current !== node) return;
        if (!label) return this.hideNow();
        this.keep = '';
        this.label = label;
      },
      destroy: () => {
        node.removeEventListener('pointerenter', enter);
        node.removeEventListener('pointerleave', leave);
        node.removeEventListener('focusin', focus);
        node.removeEventListener('focusout', leave);
        if (this.current === node) this.hideNow();
      }
    };
  };
}
