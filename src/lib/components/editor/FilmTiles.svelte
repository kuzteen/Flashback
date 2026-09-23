<script lang="ts">
  import { film, tileAt } from '$lib/filmstrip.svelte';

  let {
    startMs,
    left,
    width,
    mpp,
    viewX,
    viewW,
  }: { startMs: number; left: number; width: number; mpp: number; viewX: number; viewW: number } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);

  // Como la onda de audio, el lienzo solo cubre la parte visible del bloque: con zoom alto un
  // bloque puede medir decenas de miles de px. Las casillas se anclan al borde izquierdo del
  // bloque para que la primera muestre siempre el fotograma donde empieza el corte.
  const x0 = $derived(Math.max(0, viewX - left));
  const x1 = $derived(Math.min(width, viewX + viewW - left));

  function draw() {
    const c = canvas;
    const strip = film.strip;
    if (!c || !strip || x1 <= x0 || mpp <= 0) return;
    const h = c.clientHeight;
    const w = x1 - x0;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    const ctx = c.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    const tw = (h * strip.tileW) / strip.tileH;
    for (let k = Math.floor(x0 / tw); k * tw < x1; k++) {
      const i = tileAt(strip, startMs + k * tw * mpp);
      const sx = (i % strip.cols) * strip.tileW;
      const sy = Math.floor(i / strip.cols) * strip.tileH;
      ctx.drawImage(strip.image, sx, sy, strip.tileW, strip.tileH, k * tw - x0, 0, tw, h);
    }
  }

  $effect(() => {
    void [canvas, film.strip, startMs, mpp, x0, x1];
    draw();
  });
</script>

{#if x1 > x0}
  <canvas
    bind:this={canvas}
    class:ready={!!film.strip}
    style:left="{x0}px"
    style:width="{x1 - x0}px"
  ></canvas>
{/if}

<style>
  canvas {
    position: absolute;
    top: 0;
    height: 100%;
    display: block;
    opacity: 0;
    transition: opacity 0.25s ease;
    pointer-events: none;
  }
  canvas.ready {
    opacity: 1;
  }
</style>
