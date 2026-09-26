<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { SIZES, springFor, step, type Spring } from '$lib/switch-physics';

  // visual: solo dibuja el estado (el control es una fila entera que ya es un botón) y se inclina
  // con `pressed`, que le pasa quien la contiene.
  let {
    checked,
    onchange,
    label,
    size = 'md',
    visual = false,
    pressed = false
  }: {
    checked: boolean;
    onchange?: (v: boolean) => void;
    label: string;
    size?: 'md' | 'sm';
    visual?: boolean;
    pressed?: boolean;
  } = $props();

  const reduce = typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  const TRAVEL = springFor(reduce ? 0.2 : 0.3, reduce ? 0 : 0.25);
  const LEAN = springFor(0.2, 0);
  // Por debajo de esto sigue siendo un toque: un dedo tembloroso no arrastra.
  const DRAG_SLOP = 3;
  // Al aterrizar pasado el extremo la bola se aplasta contra la pared en vez de salirse, con tope
  // para que un golpe fuerte no la deje plana.
  const MAX_SQUASH = 4;

  const dims = $derived(SIZES[size]);

  let knob = $state<HTMLElement | null>(null);
  let fill = $state<HTMLElement | null>(null);

  // Estado del muelle fuera de la reactividad: se pinta a mano en cada fotograma.
  let target = untrack(() => checked);
  const pos: Spring = { x: target ? 1 : 0, v: 0 };
  const width: Spring = { x: 0, v: 0 };
  let widthTo = 0;
  let widthSpring = TRAVEL;
  let raf = 0;
  let last = 0;
  let gesture: { id: number; startX: number; startP: number; dragging: boolean; lastX: number; lastT: number } | null =
    null;
  let swallowClick = false;

  const clamp01 = (v: number) => Math.min(Math.max(v, 0), 1);

  function paint() {
    if (!knob || !fill) return;
    const { inner } = dims;
    const over = pos.x > 1 ? pos.x - 1 : pos.x < 0 ? -pos.x : 0;
    const w = width.x - Math.min(over * (inner - dims.knob), MAX_SQUASH);
    // La posición sale del ancho: al estirarse apagado crece hacia la derecha y encendido hacia la
    // izquierda, siempre hacia donde va a ir.
    const x = clamp01(pos.x) * (inner - w);
    const p = clamp01(pos.x);
    knob.style.width = `${w}px`;
    knob.style.transform = `translateX(${x}px)`;
    knob.style.background = `color-mix(in srgb, var(--on-accent) ${p * 100}%, var(--text-2))`;
    fill.style.opacity = String(p);
  }

  function frame(now: number) {
    const dt = last ? Math.min((now - last) / 1000, 1 / 30) : 1 / 60;
    last = now;
    let moving = false;
    if (!gesture?.dragging) moving = step(pos, target ? 1 : 0, TRAVEL, dt) || moving;
    moving = step(width, widthTo, widthSpring, dt) || moving;
    paint();
    if (moving || gesture) raf = requestAnimationFrame(frame);
    else {
      raf = 0;
      last = 0;
    }
  }

  function kick() {
    if (!raf) raf = requestAnimationFrame(frame);
  }

  function lean(on: boolean) {
    if (reduce) return;
    widthTo = on ? dims.stretched : dims.knob;
    widthSpring = on ? LEAN : TRAVEL;
    kick();
  }

  $effect(() => {
    width.x = widthTo = dims.knob;
    paint();
  });

  // Sigue los cambios hechos desde fuera; los propios ya están en marcha.
  $effect(() => {
    if (checked !== target) {
      target = checked;
      kick();
    }
  });

  $effect(() => {
    if (visual) lean(pressed);
  });

  function commit(next: boolean) {
    target = next;
    kick();
    if (next !== checked) onchange?.(next);
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0 || gesture) return;
    swallowClick = false;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    gesture = { id: e.pointerId, startX: e.clientX, startP: clamp01(pos.x), dragging: false, lastX: e.clientX, lastT: e.timeStamp };
    lean(true);
  }

  function onpointermove(e: PointerEvent) {
    const g = gesture;
    if (!g || e.pointerId !== g.id) return;
    const dx = e.clientX - g.startX;
    if (!g.dragging && Math.abs(dx) < DRAG_SLOP) return;
    g.dragging = true;
    const span = dims.inner - width.x;
    // Velocidad del gesto, para que el muelle arranque con ella al soltar.
    const dt = (e.timeStamp - g.lastT) / 1000;
    if (dt > 0) pos.v = (e.clientX - g.lastX) / span / dt;
    g.lastX = e.clientX;
    g.lastT = e.timeStamp;
    pos.x = clamp01(g.startP + dx / span);
    kick();
  }

  function endGesture(e: PointerEvent, cancel: boolean) {
    const g = gesture;
    if (!g || e.pointerId !== g.id) return;
    gesture = null;
    if (g.dragging) {
      if (cancel) kick();
      else {
        swallowClick = true;
        commit(pos.x > 0.5);
      }
    }
    lean(false);
  }

  // Toques, Espacio y Enter llegan aquí. Un arrastre termina también en click: ese no cuenta.
  function onclick() {
    if (swallowClick) {
      swallowClick = false;
      return;
    }
    commit(!target);
  }

  onDestroy(() => cancelAnimationFrame(raf));
</script>

{#if visual}
  <span
    class="switch {size}"
    aria-hidden="true"
    style:width="{dims.w}px"
    style:height="{dims.h}px"
  >
    <span class="fill" bind:this={fill}></span>
    <span class="knob" bind:this={knob} style:height="{dims.knob}px"></span>
  </span>
{:else}
  <button
    class="switch {size}"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    style:width="{dims.w}px"
    style:height="{dims.h}px"
    {onpointerdown}
    {onpointermove}
    onpointerup={(e) => endGesture(e, false)}
    onpointercancel={(e) => endGesture(e, true)}
    onkeydown={() => (swallowClick = false)}
    {onclick}
  >
    <span class="fill" bind:this={fill}></span>
    <span class="knob" bind:this={knob} style:height="{dims.knob}px"></span>
  </button>
{/if}

<style>
  .switch {
    position: relative;
    flex-shrink: 0;
    display: block;
    padding: 2px;
    border-radius: 999px;
    background: var(--bg-3);
    border: 1px solid var(--line);
    touch-action: none;
    user-select: none;
  }
  /* Área de toque algo mayor que el dibujo. */
  button.switch::after {
    content: '';
    position: absolute;
    inset: -6px -4px;
  }
  button.switch:focus-visible {
    outline: 2px solid var(--text-0);
    outline-offset: 2px;
  }
  .fill {
    position: absolute;
    inset: -1px;
    border-radius: inherit;
    background: var(--accent);
    opacity: 0;
    pointer-events: none;
  }
  .knob {
    position: relative;
    display: block;
    border-radius: 999px;
    background: var(--text-2);
    will-change: transform, width;
  }
</style>
