<script lang="ts">
  import { hoverPill } from '$lib/pill';
  import { flip } from '$lib/flip';
  import Icon from '../Icon.svelte';
  import { SHORTCUTS, comboTokens } from '$lib/shortcuts';
  import { t } from '$lib/i18n.svelte';
  import { runAction } from './actions';
  import { ui } from './ui.svelte';

  let root = $state<HTMLDivElement | null>(null);

  const keyLabel = (tok: string) => (tok === 'Space' ? t('key.space') : tok === 'Del' ? t('key.del') : tok);

  $effect(() => {
    if (!ui.toolsOpen) return;
    const onDown = (e: MouseEvent) => {
      if (root && !root.contains(e.target as Node)) ui.toolsOpen = false;
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  });
</script>

<div class="tools" bind:this={root}>
  <button
    class="trigger"
    class:open={ui.toolsOpen}
    aria-haspopup="menu"
    aria-expanded={ui.toolsOpen}
    onclick={() => (ui.toolsOpen = !ui.toolsOpen)}
  >
    <Icon name="keyboard" size={17} />
    {t('ed.tools')}
    <Icon name="chevron-down" size={13} sw={2} />
  </button>

  {#if ui.toolsOpen}
    <div class="menu" role="menu" use:flip use:hoverPill={{ selector: '.item', axis: 'y' }}>
      {#each SHORTCUTS as s (s.action)}
        <button
          role="menuitem"
          class="item"
          class:danger={s.action === 'reset'}
          onclick={() => {
            ui.toolsOpen = false;
            runAction(s.action);
          }}
        >
          <span class="keys">
            {#if s.combos.length === 0}
              <span class="none">—</span>
            {:else}
              {#each s.combos as c, i (i)}
                {#if i > 0}<span class="or">/</span>{/if}
                {#each comboTokens(c) as tok (tok)}<kbd>{keyLabel(tok)}</kbd>{/each}
              {/each}
            {/if}
          </span>
          <span class="lbl">{t(s.label)}</span>
        </button>
      {/each}
      <div class="hint">
        <span class="keys"><kbd>Alt</kbd></span>
        <span class="lbl">{t('ed.act.noSnap')}</span>
      </div>
    </div>
  {/if}
</div>

<style>
  .tools {
    position: relative;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    font-size: 13px;
    color: var(--text-1);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, border-color 0.14s ease;
  }
  .trigger:hover,
  .trigger.open {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .trigger > :global(svg:last-child) {
    transition: transform 0.2s ease;
  }
  .trigger.open > :global(svg:last-child) {
    transform: rotate(180deg);
  }
  .menu {
    position: absolute;
    left: 0;
    bottom: calc(100% + 8px);
    min-width: 340px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    z-index: 70;
  }
  .item,
  .hint {
    display: grid;
    grid-template-columns: 150px 1fr;
    align-items: center;
    gap: 12px;
    padding: 7px 10px;
    font-size: 13px;
    text-align: left;
    color: var(--text-1);
    border-radius: 6px;
  }
  .item:hover {
    color: var(--text-0);
  }
  .menu > :global(.slide-pill) {
    background: var(--bg-3);
    border-radius: 6px;
  }
  .item.danger .lbl {
    color: var(--rec);
  }
  .hint {
    margin-top: 4px;
    padding-top: 9px;
    border-top: 1px solid var(--line);
    border-radius: 0;
    color: var(--text-3);
  }
  .keys {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }
  kbd {
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    display: inline-grid;
    place-items: center;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-0);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-bottom-width: 2px;
    border-radius: 5px;
  }
  .or,
  .none {
    font-size: 11px;
    color: var(--text-3);
  }
</style>
