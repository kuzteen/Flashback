<script lang="ts">
  import { untrack, type Snippet } from 'svelte';
  import MorphTip from '../MorphTip.svelte';
  import Icon from '../Icon.svelte';
  import { TipGroup } from '$lib/morph-tip.svelte';

  let { side, open, label, icon, ontoggle, children }: {
    side: 'left' | 'right';
    open: boolean;
    label: string;
    icon: string;
    ontoggle: () => void;
    children: Snippet;
  } = $props();

  const id = $props.id();
  const tips = new TipGroup(untrack(() => side === 'left' ? 'right' : 'left'));
  let button = $state<HTMLButtonElement | null>(null);
  let content = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (!open && content?.contains(document.activeElement)) button?.focus({ preventScroll: true });
  });
</script>

<!-- El cuerpo permanece montado: CSS puede invertir una transición en curso sin saltar ni
     reiniciar los controles. Cerrado queda fuera del foco y no recibe interacción. -->
<aside class="panel" class:open class:right={side === 'right'} aria-label={label}>
  <div class="sheet">
    <button class="toggle" class:on={open} bind:this={button}
      aria-label={label} aria-expanded={open} aria-controls={id}
      use:tips.trigger={label} onclick={ontoggle}
    >
      <Icon name={icon} size={18} />
    </button>
    <div class="content" id={id} bind:this={content} inert={!open} aria-hidden={!open}>
      {@render children()}
    </div>
  </div>
</aside>
<MorphTip group={tips} />

<style>
  .panel {
    position: relative;
    flex: none;
    width: 0;
    min-width: 0;
    z-index: 5;
    transition: width var(--panel-duration) var(--panel-ease);
  }
  .panel.open { width: var(--format-w); }
  .sheet {
    position: absolute;
    inset: 0 auto 0 0;
    width: var(--format-w);
    pointer-events: none;
    background: transparent;
    clip-path: inset(0 calc(100% - 60px) 0 0);
    transition: clip-path var(--panel-duration) var(--panel-ease), background-color 180ms ease;
  }
  .right .sheet { left: auto; right: 0; clip-path: inset(0 0 0 calc(100% - 60px)); }
  .open .sheet { clip-path: inset(0); background: var(--base); }
  .toggle {
    position: absolute;
    top: 12px;
    left: 12px;
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    pointer-events: auto;
    color: var(--text-1);
    background: var(--glass);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: color 140ms ease, background-color 180ms ease, border-color 180ms ease;
  }
  .right .toggle { left: auto; right: 12px; }
  .toggle.on { color: var(--text-0); background: var(--bg-hover); border-color: transparent; }
  .toggle:hover { color: var(--text-0); background: var(--bg-hover); }
  .content {
    position: absolute;
    inset: 62px 0 0;
    padding: 0 3px 14px 14px;
    overflow: hidden auto;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
    opacity: 0;
    visibility: hidden;
    translate: -12px 0;
    transition: opacity 180ms ease, translate var(--panel-duration) var(--panel-ease), visibility 0s var(--panel-duration);
  }
  .right .content { translate: 12px 0; }
  .open .content {
    pointer-events: auto;
    opacity: 1;
    visibility: visible;
    translate: 0 0;
    transition-delay: 0s;
  }
  @media (prefers-reduced-motion: reduce) {
    .panel, .sheet, .content, .toggle { transition: none; }
  }
</style>
