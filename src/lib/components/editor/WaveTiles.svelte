<script lang="ts">
  import { editorState } from '$lib/editor-state.svelte';

  let {
    peaks,
    startMs,
    left,
    width,
    mpp,
    viewX,
    viewW,
    vol,
    dim,
  }: {
    peaks: number[] | null;
    startMs: number;
    left: number;
    width: number;
    mpp: number;
    viewX: number;
    viewW: number;
    vol: number;
    dim: boolean;
  } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);

  // Como la tira de fotogramas: el lienzo solo cubre la parte visible del bloque, porque con zoom
  // alto un bloque puede medir decenas de miles de px.
  const x0 = $derived(Math.max(0, viewX - left));
  const x1 = $derived(Math.min(width, viewX + viewW - left));

  function draw() {
    const c = canvas;
    const p = peaks;
    const dur = editorState.durationMs;
    if (!c || !p || x1 <= x0 || mpp <= 0 || dur <= 0) return;
    const w = Math.round(x1 - x0);
    const h = c.clientHeight;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    const ctx = c.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    const mid = h / 2;
    const amp = h * 0.4;
    const path = new Path2D();
    for (let x = 0; x < w; x++) {
      const src = startMs + (x0 + x) * mpp;
      const b = Math.min(p.length - 1, Math.max(0, Math.floor((src / dur) * p.length)));
      const y = Math.max(0.5, Math.min(1, p[b] * 1.8) * amp * vol);
      path.rect(x, mid - y, 1, y * 2);
    }
    // El lienzo no entiende var(): los colores se leen de los tokens del tema al pintar.
    ctx.fillStyle = getComputedStyle(c).getPropertyValue(dim ? '--wave-off' : '--wave').trim();
    ctx.fill(path);
  }

  $effect(() => {
    void [canvas, peaks, startMs, mpp, x0, x1, vol, dim];
    draw();
  });
</script>

{#if x1 > x0 && peaks}
  <canvas bind:this={canvas} style:left="{x0}px" style:width="{x1 - x0}px"></canvas>
{/if}

<style>
  canvas {
    position: absolute;
    top: 0;
    height: 100%;
    display: block;
    pointer-events: none;
  }
</style>
