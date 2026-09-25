// Menús y desplegables anclados a su botón (position: absolute). Si al abrirse no caben dentro de
// la ventana por su lado y por el otro hay más sitio, se colocan en espejo respecto al botón, con
// la misma separación: el que abre hacia abajo pasa a abrir hacia arriba y al revés. Se vuelve a
// medir si cambia de alto (resultados de un buscador), y en el mismo fotograma, así que no salta.
// Los menús con position: fixed ya se colocan solos y se dejan como están.
const EDGE = 8;

type Box = { top: number; bottom: number };

export function flipTo(menu: Box, anchor: Box, viewportH: number): { side: 'up' | 'down'; gap: number } | null {
  if (menu.top >= anchor.bottom - 1) {
    const gap = menu.top - anchor.bottom;
    const below = viewportH - EDGE - menu.top;
    const above = anchor.top - gap - EDGE;
    return menu.bottom > viewportH - EDGE && above > below ? { side: 'up', gap } : null;
  }
  if (menu.bottom <= anchor.top + 1) {
    const gap = anchor.top - menu.bottom;
    const above = menu.bottom - EDGE;
    const below = viewportH - EDGE - anchor.bottom - gap;
    return menu.top < EDGE && below > above ? { side: 'down', gap } : null;
  }
  return null;
}

export function flip(node: HTMLElement) {
  if (getComputedStyle(node).position !== 'absolute') return;

  const place = () => {
    node.style.top = '';
    node.style.bottom = '';
    node.style.transformOrigin = '';
    const parent = node.offsetParent;
    if (!parent) return;
    const to = flipTo(node.getBoundingClientRect(), parent.getBoundingClientRect(), window.innerHeight);
    if (!to) return;
    const originX = getComputedStyle(node).transformOrigin.split(' ')[0];
    if (to.side === 'up') {
      node.style.top = 'auto';
      node.style.bottom = `calc(100% + ${to.gap}px)`;
      node.style.transformOrigin = `${originX} 100%`;
    } else {
      node.style.bottom = 'auto';
      node.style.top = `calc(100% + ${to.gap}px)`;
      node.style.transformOrigin = `${originX} 0`;
    }
  };

  place();
  const ro = new ResizeObserver(place);
  ro.observe(node);
  return { destroy: () => ro.disconnect() };
}
