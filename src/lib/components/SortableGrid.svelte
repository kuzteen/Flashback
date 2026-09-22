<script lang="ts" generics="T">
  import { untrack, tick, type Snippet } from 'svelte';

  // Rejilla de tarjetas de la biblioteca y de las playlists. La tarjeta la pone quien la usa;
  // aquí solo viven la virtualización y el arrastre. onreorder recibe el índice de origen y el
  // hueco de destino (0..n, antes de sacar el elemento): sin él la rejilla no se arrastra.
  let {
    items,
    key,
    children,
    onreorder
  }: {
    items: T[];
    key: (item: T) => string;
    children: Snippet<[T]>;
    onreorder?: (from: number, to: number) => void;
  } = $props();

  // Virtualización: solo se montan las filas visibles más un colchón. El hueco de las filas
  // que faltan va como padding del contenedor, no como divs espaciadores, porque en una
  // display:grid un div de relleno ocuparía celda y descuadraría las columnas.
  const BUFFER_ROWS = 2;
  const INITIAL = 24;

  let gridEl = $state<HTMLElement | null>(null);
  let rowH = $state(0);
  let cols = $state(2);
  let start = $state(0);
  let end = $state(INITIAL);
  let scroller: HTMLElement | null = null;

  const visible = $derived(items.slice(start, end));
  const totalRows = $derived(Math.ceil(items.length / cols));
  const padTop = $derived(Math.floor(start / cols) * rowH);
  const padBottom = $derived(Math.max(0, totalRows - Math.ceil(end / cols)) * rowH);

  function findScroller(el: HTMLElement): HTMLElement {
    let p = el.parentElement;
    while (p) {
      const oy = getComputedStyle(p).overflowY;
      if (oy === 'auto' || oy === 'scroll') return p;
      p = p.parentElement;
    }
    return document.documentElement;
  }

  // Las columnas y la altura de fila se leen del DOM en vez de asumirlas: gridTemplateColumns
  // llega ya resuelto a anchos, así que el cálculo sobrevive a cualquier breakpoint nuevo.
  function measure() {
    if (!gridEl) return;
    const cs = getComputedStyle(gridEl);
    cols = cs.gridTemplateColumns.split(' ').filter(Boolean).length || 1;
    const cell = gridEl.querySelector<HTMLElement>('.cell');
    if (cell) rowH = cell.offsetHeight + (parseFloat(cs.rowGap) || 0);
  }

  function update() {
    if (!gridEl || !scroller || rowH <= 0) return;
    const gridTop =
      gridEl.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top +
      scroller.scrollTop;
    const into = scroller.scrollTop - gridTop;
    const rowsFit = Math.ceil(scroller.clientHeight / rowH) + BUFFER_ROWS * 2 + 1;
    const maxStart = Math.max(0, (Math.ceil(items.length / cols) - 1) * cols);
    const nextStart = Math.min(maxStart, Math.max(0, Math.floor(into / rowH) - BUFFER_ROWS) * cols);
    const nextEnd = Math.min(items.length, nextStart + rowsFit * cols);
    if (nextStart !== start) start = nextStart;
    if (nextEnd !== end) end = nextEnd;
  }

  $effect(() => {
    const el = gridEl;
    if (!el) return;
    scroller = findScroller(el);
    untrack(() => {
      measure();
      update();
    });
    let ticking = false;
    const onScroll = () => {
      if (ticking) return;
      ticking = true;
      requestAnimationFrame(() => {
        ticking = false;
        update();
      });
    };
    scroller.addEventListener('scroll', onScroll, { passive: true });
    // Solo se observa el scroller: la rejilla cambia de alto cada vez que ajustamos su
    // padding, y observarla realimentaría el propio cálculo.
    const ro = new ResizeObserver(() => {
      measure();
      update();
    });
    ro.observe(scroller);
    const sc = scroller;
    return () => {
      sc.removeEventListener('scroll', onScroll);
      ro.disconnect();
    };
  });

  $effect(() => {
    items.length;
    untrack(() => {
      measure();
      update();
    });
  });

  // Arrastre para reordenar. Las demás tarjetas no se apartan mientras se arrastra: solo una línea
  // marca el hueco y todo se recoloca de una vez al soltar. Con la rejilla bailando bajo el
  // puntero el destino cambiaba solo y costaba acertar. El hueco sale de la geometría de la
  // rejilla y no del DOM, así que funciona igual sobre filas que la virtualización no ha montado.
  const DRAG_PX = 6;
  const EDGE_PX = 72;
  const MAX_SCROLL = 22;

  let dragId = $state<string | null>(null);
  let line = $state<{ x: number; y: number; h: number } | null>(null);
  let press: { x: number; y: number; index: number; cell: HTMLElement } | null = null;
  let from = -1;
  let target = -1;
  let lastX = 0;
  let lastY = 0;
  let offX = 0;
  let offY = 0;
  let gap = 0;
  let ghost: HTMLElement | null = null;
  let raf = 0;

  // La recolocación al soltar se anima a mano (FLIP con la Web Animations API) y no con
  // animate: de Svelte. animate mide con getBoundingClientRect cada tarjeta montada en cada
  // cambio de la lista, también los que provoca la virtualización al hacer scroll; ese layout
  // forzado a mitad de reconciliación, con el padding ya cambiado y las filas aún sin insertar,
  // hacía que el scroll anchoring de Chromium devolviera la página hacia abajo al subir.
  // La tarjeta movida sale desde donde estaba el fantasma y no desde su hueco antiguo: así
  // parece que es el mismo objeto el que aterriza.
  const reduceMotion =
    typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  async function reorderAnimated(f: number, to: number, landed?: { id: string; rect: DOMRect }) {
    if (!onreorder) return;
    if (reduceMotion || !gridEl) {
      onreorder(f, to);
      return;
    }
    const before = new Map<string, DOMRect>();
    for (const cell of gridEl.querySelectorAll<HTMLElement>('.cell')) {
      before.set(cell.dataset.id!, cell.getBoundingClientRect());
    }
    if (landed) before.set(landed.id, landed.rect);
    onreorder(f, to);
    await tick();
    if (!gridEl) return;
    for (const cell of gridEl.querySelectorAll<HTMLElement>('.cell')) {
      const a = before.get(cell.dataset.id!);
      if (!a) continue;
      const b = cell.getBoundingClientRect();
      const dx = a.left - b.left;
      const dy = a.top - b.top;
      const sx = a.width / b.width;
      const sy = a.height / b.height;
      if (!dx && !dy && sx === 1 && sy === 1) continue;
      cell.animate(
        [
          { transformOrigin: 'top left', transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})` },
          { transformOrigin: 'top left', transform: 'none' }
        ],
        { duration: 260, easing: 'cubic-bezier(0.215, 0.61, 0.355, 1)' }
      );
    }
  }

  function onPointerDown(e: PointerEvent) {
    if (!onreorder || e.button !== 0 || dragId) return;
    const el = e.target as HTMLElement;
    if (el.closest('button, input, textarea, a, [role="menu"]')) return;
    const cell = el.closest<HTMLElement>('.cell');
    if (!cell) return;
    press = { x: e.clientX, y: e.clientY, index: Number(cell.dataset.index), cell };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', stop);
  }

  function begin() {
    if (!press || !gridEl) return;
    const card = press.cell.firstElementChild as HTMLElement | null;
    if (!card) return;
    const r = card.getBoundingClientRect();
    offX = press.x - r.left;
    offY = press.y - r.top;
    gap = parseFloat(getComputedStyle(gridEl).columnGap) || 0;
    ghost = card.cloneNode(true) as HTMLElement;
    ghost.querySelectorAll('video').forEach((v) => v.remove());
    ghost.classList.remove('open');
    ghost.classList.add('drag-ghost');
    ghost.removeAttribute('tabindex');
    ghost.setAttribute('aria-hidden', 'true');
    ghost.style.width = `${r.width}px`;
    ghost.style.height = `${r.height}px`;
    ghost.style.transformOrigin = `${offX}px ${offY}px`;
    document.body.appendChild(ghost);
    moveGhost();
    const g = ghost;
    requestAnimationFrame(() => g.classList.add('lifted'));
    from = press.index;
    dragId = from >= 0 && from < items.length ? key(items[from]) : null;
    document.body.classList.add('grid-dragging');
    window.getSelection()?.removeAllRanges();
    window.addEventListener('keydown', onEscape, true);
    raf = requestAnimationFrame(autoscroll);
  }

  function onMove(e: PointerEvent) {
    lastX = e.clientX;
    lastY = e.clientY;
    if (!dragId) {
      if (!press || Math.hypot(lastX - press.x, lastY - press.y) < DRAG_PX) return;
      begin();
      if (!dragId) return;
    }
    moveGhost();
    locate();
  }

  function moveGhost() {
    if (ghost) ghost.style.translate = `${lastX - offX}px ${lastY - offY}px`;
  }

  function locate() {
    if (!gridEl || rowH <= 0) return;
    const n = items.length;
    const r = gridEl.getBoundingClientRect();
    const colW = (r.width - gap * (cols - 1)) / cols;
    const row = Math.max(0, Math.floor((lastY - r.top) / rowH));
    const x = Math.min(Math.max(lastX - r.left, 0), r.width - 1);
    const col = Math.min(cols - 1, Math.floor(x / (colW + gap)));
    let h = row * cols + col;
    let after = x - col * (colW + gap) > colW / 2;
    if (h >= n) {
      h = n - 1;
      after = true;
    }
    const slot = h + (after ? 1 : 0);
    // Soltar a cualquier lado de la propia tarjeta la deja donde estaba: sin línea que prometa
    // un movimiento que no va a pasar.
    if (slot === from || slot === from + 1) {
      target = -1;
      line = null;
      return;
    }
    target = slot;
    const hc = h % cols;
    line = {
      x: hc * (colW + gap) + (after ? colW + gap / 2 : -gap / 2),
      y: Math.floor(h / cols) * rowH,
      h: rowH - (parseFloat(getComputedStyle(gridEl).rowGap) || 0)
    };
  }

  // Cerca del borde del scroller la página corre sola, más deprisa cuanto más se acerca el
  // puntero: sin esto no hay forma de llevar una tarjeta a una fila que no está en pantalla.
  function autoscroll() {
    raf = requestAnimationFrame(autoscroll);
    const sc = scroller;
    if (!sc) return;
    const top = sc === document.documentElement ? 0 : sc.getBoundingClientRect().top;
    const bottom =
      sc === document.documentElement ? window.innerHeight : sc.getBoundingClientRect().bottom;
    let dy = 0;
    if (lastY < top + EDGE_PX) dy = -(top + EDGE_PX - lastY);
    else if (lastY > bottom - EDGE_PX) dy = lastY - (bottom - EDGE_PX);
    if (!dy) return;
    const step = Math.sign(dy) * Math.min(MAX_SCROLL, Math.ceil((Math.abs(dy) / EDGE_PX) * MAX_SCROLL));
    const before = sc.scrollTop;
    sc.scrollTop += step;
    if (sc.scrollTop !== before) locate();
  }

  // El click que el navegador sintetiza al soltar abriría el editor de la tarjeta de debajo.
  function swallowClick() {
    const eat = (e: Event) => {
      e.stopPropagation();
      e.preventDefault();
    };
    window.addEventListener('click', eat, { capture: true, once: true });
    setTimeout(() => window.removeEventListener('click', eat, true), 0);
  }

  function onUp() {
    const dragged = !!dragId;
    const movedId = dragId;
    const f = from;
    const s = target;
    const rect = ghost?.getBoundingClientRect() ?? null;
    stop();
    if (!dragged) return;
    swallowClick();
    if (s < 0) return;
    reorderAnimated(f, s, movedId && rect ? { id: movedId, rect } : undefined);
  }

  function onEscape(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopPropagation();
    stop();
  }

  function stop() {
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
    window.removeEventListener('pointercancel', stop);
    window.removeEventListener('keydown', onEscape, true);
    cancelAnimationFrame(raf);
    ghost?.remove();
    ghost = null;
    document.body.classList.remove('grid-dragging');
    press = null;
    dragId = null;
    line = null;
    from = -1;
    target = -1;
  }

  $effect(() => () => stop());

  // Alt + flechas mueve la tarjeta con foco: el mismo reordenado sin ratón. El foco se repone
  // tras mover porque el nodo se reinserta en el DOM y Chromium lo suelta por el camino.
  async function onKeyDown(e: KeyboardEvent) {
    if (!onreorder || !e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
    const card = e.target as HTMLElement;
    const cell = card.parentElement;
    if (!cell?.classList.contains('cell')) return;
    const i = Number(cell.dataset.index);
    const step =
      e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : e.key === 'ArrowUp' ? -cols : e.key === 'ArrowDown' ? cols : 0;
    if (!step) return;
    e.preventDefault();
    const to = Math.max(0, Math.min(items.length - 1, i + step));
    if (to === i) return;
    const id = key(items[i]);
    await reorderAnimated(i, to > i ? to + 1 : to);
    gridEl?.querySelector<HTMLElement>(`.cell[data-id="${CSS.escape(id)}"] > *`)?.focus();
  }
</script>

<div
  class="grid"
  class:dragging={!!dragId}
  bind:this={gridEl}
  style:padding-top="{padTop}px"
  style:padding-bottom="{padBottom}px"
  role="presentation"
  onpointerdown={onPointerDown}
  onkeydown={onKeyDown}
>
  {#each visible as item, i (key(item))}
    <div class="cell" class:src={key(item) === dragId} data-index={start + i} data-id={key(item)}>
      {@render children(item)}
    </div>
  {/each}
  {#if line}
    <div class="drop-line" style:left="{line.x}px" style:top="{line.y}px" style:height="{line.h}px"></div>
  {/if}
</div>

<style>
  .grid {
    position: relative;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 20px;
  }
  @media (min-width: 1500px) {
    .grid {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }
  .cell {
    min-width: 0;
    transition: opacity 0.16s ease;
  }
  .cell.src {
    opacity: 0.3;
  }
  .drop-line {
    position: absolute;
    width: 4px;
    margin-left: -2px;
    border-radius: 2px;
    background: var(--accent);
    box-shadow: 0 0 12px var(--accent);
    pointer-events: none;
  }
  /* Mientras se arrastra, las tarjetas no reciben el puntero: el destino se calcula por
     geometría, y así no se encienden las vistas previas de vídeo de todo lo que se cruza. */
  .grid.dragging .cell {
    pointer-events: none;
  }
  :global(body.grid-dragging),
  :global(body.grid-dragging *) {
    cursor: grabbing !important;
  }
  :global(.drag-ghost) {
    position: fixed !important;
    left: 0;
    top: 0;
    margin: 0;
    z-index: 1000;
    pointer-events: none;
    opacity: 0.95;
    box-shadow:
      0 0 0 2px rgba(255, 255, 255, 0.22),
      0 26px 60px -14px rgba(0, 0, 0, 0.85) !important;
    transition: scale 0.18s cubic-bezier(0.2, 0.8, 0.2, 1), rotate 0.18s cubic-bezier(0.2, 0.8, 0.2, 1);
    will-change: translate;
  }
  :global(.drag-ghost.lifted) {
    scale: 0.55;
    rotate: -2deg;
  }
</style>
