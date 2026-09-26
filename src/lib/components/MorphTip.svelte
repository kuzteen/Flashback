<script lang="ts">
  import { cubicOut } from 'svelte/easing';
  import type { TipGroup } from '$lib/morph-tip.svelte';
  import { placeTip } from '$lib/tip-math';

  let { group }: { group: TipGroup } = $props();

  // El texto nuevo entra desde el lado del que viene el globo y el viejo se va hacia el otro, más
  // rápido y más corto. El desenfoque evita ver dos textos casi iguales nítidos a la vez (50 MB y
  // 100 MB), que se leía como un parpadeo.
  const IN = { d: 10, blur: 4, ms: 200 };
  const OUT = { d: -6, blur: 2, ms: 120, leaving: true };
  const reduce = typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches;

  let measure = $state<HTMLSpanElement | null>(null);
  let size = $state({ w: 0, h: 0 });
  let vw = $state(0);
  let vh = $state(0);

  // El ancho de destino sale de una copia oculta del texto, así el globo sabe a qué tamaño ir
  // antes de que el texto nuevo termine de entrar. +2 por el borde.
  $effect(() => {
    void group.label;
    if (measure) size = { w: measure.offsetWidth + 2, h: measure.offsetHeight + 2 };
  });

  const place = $derived(
    group.anchor && size.w ? placeTip(group.anchor, size, group.side, { w: vw, h: vh }) : null
  );

  const still = $derived(group.fresh || reduce);
  // El texto que sale se saca del flujo para quedar encima del que entra, en el mismo sitio.
  function drift(_: Element, { d, blur, ms, leaving }: { d: number; blur: number; ms: number; leaving?: boolean }) {
    const dist = d * group.dir;
    const axis = group.side === 'right' ? 'Y' : 'X';
    const pin = leaving ? 'position: absolute; left: 0; top: 0;' : '';
    return {
      duration: still ? 0 : ms,
      easing: cubicOut,
      css: (t: number, u: number) =>
        `${pin} opacity: ${t}; transform: translate${axis}(${u * dist}px); filter: blur(${u * blur}px)`
    };
  }

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }
</script>

<svelte:window bind:innerWidth={vw} bind:innerHeight={vh} />

<div
  use:portal
  class="tip {group.side}"
  class:open={group.open && !!place}
  class:fresh={group.fresh}
  aria-hidden="true"
  style:transform={place ? `translate(${place.x}px, ${place.y}px)` : undefined}
  style:width="{size.w}px"
  style:height="{size.h}px"
  style:--caret="{place?.caret ?? 0}px"
>
  <span class="caret"></span>
  <span class="clip">
    <span class="tip-text"
      >{group.keep}<span class="swap"
        >{#key group.seq}<span class="part" in:drift={IN} out:drift={OUT}
            >{group.label.slice(group.keep.length)}</span
          >{/key}</span
      ></span
    >
  </span>
  <span class="tip-text measure" bind:this={measure}>{group.label}</span>
</div>

<style>
  .tip {
    --morph: cubic-bezier(0.23, 1, 0.32, 1);
    position: fixed;
    left: 0;
    top: 0;
    z-index: 1000;
    box-sizing: border-box;
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-float);
    pointer-events: none;
    opacity: 0;
    visibility: hidden;
    transition:
      opacity 0.14s ease,
      visibility 0.14s,
      translate 0.18s var(--morph),
      transform 0.3s var(--morph),
      width 0.3s var(--morph),
      height 0.3s var(--morph);
  }
  .tip.right {
    translate: -4px 0;
  }
  .tip.bottom {
    translate: 0 -4px;
  }
  .tip.top {
    translate: 0 4px;
  }
  .tip.open {
    opacity: 1;
    visibility: visible;
    translate: 0 0;
  }
  /* Al aparecer desde cerrado se coloca en su sitio sin viajar desde el anterior. */
  .tip.fresh {
    transition:
      opacity 0.14s ease,
      visibility 0.14s,
      translate 0.18s var(--morph);
  }
  .caret {
    position: absolute;
    width: 8px;
    height: 8px;
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    transform: rotate(45deg);
    transition: top 0.3s var(--morph), left 0.3s var(--morph);
  }
  .right .caret {
    left: -5px;
    top: calc(var(--caret) - 5px);
    border-top-color: transparent;
    border-right-color: transparent;
  }
  .bottom .caret {
    top: -5px;
    left: calc(var(--caret) - 5px);
    border-right-color: transparent;
    border-bottom-color: transparent;
  }
  .top .caret {
    bottom: -5px;
    left: calc(var(--caret) - 5px);
    border-top-color: transparent;
    border-left-color: transparent;
  }
  .fresh .caret {
    transition: none;
  }
  .clip {
    position: absolute;
    inset: 0;
    overflow: hidden;
    border-radius: inherit;
  }
  .tip-text {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    padding: 8px 13px;
    font-family: var(--font-ui);
    font-size: 13.5px;
    font-weight: 500;
    line-height: 1.3;
    letter-spacing: 0;
    white-space: pre;
    color: var(--text-0);
  }
  .swap {
    position: relative;
    display: inline-block;
  }
  .part {
    display: inline-block;
  }
  .measure {
    inset: auto;
    width: max-content;
    visibility: hidden;
  }
  @media (prefers-reduced-motion: reduce) {
    .tip,
    .caret {
      transition: opacity 0.14s ease, visibility 0.14s;
    }
  }
</style>
