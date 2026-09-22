<script lang="ts">
  import Icon from '../Icon.svelte';
  import { clipOrder, editorState, navigateClip } from '$lib/editor-state.svelte';
  import { formatSize } from '$lib/clips';
  import { localeTag, t } from '$lib/i18n.svelte';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { runAction } from './actions';

  let { onclose }: { onclose: () => void } = $props();

  const navIdx = $derived(clipOrder.list.findIndex((c) => c.id === editorState.clip?.id));
  const hasPrev = $derived(navIdx > 0);
  const hasNext = $derived(navIdx >= 0 && navIdx < clipOrder.list.length - 1);
  const date = $derived.by(() => {
    const d = editorState.clip?.createdAt;
    if (!d) return '';
    try {
      return d.toLocaleDateString(localeTag(), { day: 'numeric', month: 'short', year: 'numeric' });
    } catch {
      return '';
    }
  });

  async function reveal() {
    const p = editorState.clip?.path;
    if (!p) return;
    try {
      await revealItemInDir(p);
    } catch (err) {
      console.error('revealItemInDir', err);
    }
  }
</script>

<header class="head">
  <div class="nav">
    <button
      class="ib prev"
      aria-label={t('ed.prevClip')}
      data-tip={t('ed.prevClip')}
      data-tip-pos="below"
      data-tip-align="start"
      disabled={!hasPrev || editorState.exporting}
      onclick={() => navigateClip(-1)}
    >
      <Icon name="chevron-down" size={18} sw={2.2} />
    </button>
    <button
      class="ib next"
      aria-label={t('ed.nextClip')}
      data-tip={t('ed.nextClip')}
      data-tip-pos="below"
      disabled={!hasNext || editorState.exporting}
      onclick={() => navigateClip(1)}
    >
      <Icon name="chevron-down" size={18} sw={2.2} />
    </button>
  </div>

  <div class="titles">
    <h2 class="title">{editorState.clip?.title ?? ''}</h2>
    <div class="meta mono">
      <span>{date}</span>
      <span class="dot">•</span>
      <span>{formatSize(editorState.clip?.sizeBytes ?? 0)}</span>
      <span class="dot">•</span>
      <button class="link" onclick={reveal} disabled={!editorState.clip?.path}>{t('ed.showInFolder')}</button>
    </div>
  </div>

  <div class="actions">
    <button
      class="ib"
      aria-label={t('ed.act.undo')}
      data-tip="{t('ed.act.undo')} · Ctrl+Z"
      data-tip-pos="below"
      disabled={!editorState.canUndo}
      onclick={() => runAction('undo')}
    >
      <Icon name="undo" size={18} />
    </button>
    <button
      class="ib"
      aria-label={t('ed.act.redo')}
      data-tip="{t('ed.act.redo')} · Ctrl+Shift+Z"
      data-tip-pos="below"
      disabled={!editorState.canRedo}
      onclick={() => runAction('redo')}
    >
      <Icon name="redo" size={18} />
    </button>
    <span class="sep"></span>
    <button
      class="ib"
      aria-label={t('ed.closeEditor')}
      data-tip={t('ed.closeEditor')}
      data-tip-pos="below"
      data-tip-align="end"
      onclick={onclose}
    >
      <Icon name="close" size={18} sw={2} />
    </button>
  </div>
</header>

<style>
  .head {
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    height: 56px;
    padding: 0 14px 0 10px;
    background: var(--base);
    border-bottom: 1px solid var(--line);
  }
  .nav {
    display: flex;
    gap: 2px;
  }
  .ib {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    color: var(--text-1);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease;
  }
  .ib:hover:not(:disabled) {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  .ib:disabled {
    opacity: 0.3;
  }
  /* El chevron del set apunta hacia abajo; girarlo evita meter dos iconos más. */
  .prev :global(svg) {
    transform: rotate(90deg);
  }
  .next :global(svg) {
    transform: rotate(-90deg);
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .title {
    font-family: var(--font-display);
    font-size: 16px;
    font-weight: 600;
    color: var(--text-0);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
    letter-spacing: 0.04em;
    color: var(--text-3);
    white-space: nowrap;
  }
  .dot {
    color: var(--text-3);
  }
  .link {
    font: inherit;
    letter-spacing: inherit;
    color: var(--text-2);
    padding: 0;
    transition: color 0.14s ease;
  }
  .link:hover:not(:disabled) {
    color: var(--text-0);
    text-decoration: underline;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .sep {
    width: 1px;
    height: 20px;
    margin: 0 8px;
    background: var(--line-strong);
  }
</style>
