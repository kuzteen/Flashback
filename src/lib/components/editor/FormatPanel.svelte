<script lang="ts">
  import Icon from '../Icon.svelte';
  import { commit, editorState } from '$lib/editor-state.svelte';
  import type { OutputFormat } from '$lib/edit-model';
  import { t } from '$lib/i18n.svelte';
  import { ui } from './ui.svelte';

  const format = $derived(editorState.edit.format);

  // Cada cambio de formato es un paso de deshacer; elegir el que ya está no ensucia el historial.
  function set(next: OutputFormat) {
    if (JSON.stringify(next) === JSON.stringify(format)) return;
    commit({ ...editorState.edit, format: next });
  }
</script>

<aside class="panel" class:open={ui.formatOpen}>
  <button
    class="toggle"
    class:on={ui.formatOpen}
    aria-label={t('ed.format')}
    aria-expanded={ui.formatOpen}
    data-tip={t('ed.format')}
    data-tip-pos="below"
    data-tip-align="end"
    onclick={() => ui.toggleFormat()}
  >
    <Icon name="crop" size={18} />
  </button>

  {#if ui.formatOpen}
    <div class="body">
      <h3 class="title">{t('ed.format')}</h3>
      <button class="opt" class:on={format.kind === 'horizontal'} onclick={() => set({ kind: 'horizontal' })}>
        <span class="shape h"></span>
        <span class="txt">
          <span class="name">{t('ed.fmtHorizontal')}</span>
          <span class="hint">{t('ed.fmtHorizontalHint')}</span>
        </span>
      </button>
      <button
        class="opt"
        class:on={format.kind === 'vertical'}
        onclick={() => set({ kind: 'vertical', fill: format.kind === 'vertical' ? format.fill : 'crop' })}
      >
        <span class="shape v"></span>
        <span class="txt">
          <span class="name">{t('ed.fmtVertical')}</span>
          <span class="hint">{t('ed.fmtVerticalHint')}</span>
        </span>
      </button>

      {#if format.kind === 'vertical'}
        <div class="seg" role="radiogroup" aria-label={t('ed.fill')}>
          <button
            role="radio"
            aria-checked={format.fill === 'crop'}
            class:on={format.fill === 'crop'}
            onclick={() => set({ kind: 'vertical', fill: 'crop' })}
          >
            {t('ed.fillCrop')}
          </button>
          <button
            role="radio"
            aria-checked={format.fill === 'fit'}
            class:on={format.fill === 'fit'}
            onclick={() => set({ kind: 'vertical', fill: 'fit' })}
          >
            {t('ed.fillFit')}
          </button>
        </div>
        <p class="note">{format.fill === 'crop' ? t('ed.cropHint') : t('ed.fitHint')}</p>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .panel {
    flex: none;
    width: 52px;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 14px;
    padding: 10px 8px;
    background: var(--bg-0);
    border-left: 1px solid var(--line);
    transition: width 0.18s ease;
    overflow: hidden;
  }
  .panel.open {
    width: 260px;
    padding: 10px 14px 14px;
  }
  .toggle {
    align-self: flex-end;
    width: 36px;
    height: 36px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease;
  }
  .toggle:hover,
  .toggle.on {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 230px;
  }
  .title {
    margin-bottom: 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px;
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    transition: border-color 0.14s ease, background 0.14s ease;
  }
  .opt:hover {
    background: var(--bg-2);
  }
  .opt.on {
    border-color: var(--text-3);
    background: var(--bg-2);
  }
  /* Miniatura de la proporción: se entiende antes que el texto. */
  .shape {
    flex: none;
    border: 2px solid var(--text-3);
    border-radius: 3px;
  }
  .opt.on .shape {
    border-color: var(--text-0);
  }
  .shape.h {
    width: 28px;
    height: 16px;
  }
  .shape.v {
    width: 14px;
    height: 24px;
    margin: 0 7px;
  }
  .txt {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .name {
    font-size: 13px;
    color: var(--text-0);
  }
  .hint {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .seg {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
    padding: 3px;
    margin-top: 4px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
  }
  .seg button {
    height: 30px;
    font-size: 12.5px;
    color: var(--text-2);
    border-radius: 4px;
    transition: color 0.14s ease, background 0.14s ease;
  }
  .seg button:hover {
    color: var(--text-0);
  }
  .seg button.on {
    color: var(--text-0);
    background: var(--bg-3);
  }
  .note {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-3);
  }
</style>
