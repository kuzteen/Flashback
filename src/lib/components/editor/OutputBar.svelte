<script lang="ts">
  import { flip } from '$lib/flip';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '../Icon.svelte';
  import WatermarkToggle from '../WatermarkToggle.svelte';
  import { editorState, exportClip, shareEdit, type ExportFormat } from '$lib/editor-state.svelte';
  import { CANCELLED, openShare } from '$lib/share.svelte';
  import { refreshLibrary } from '$lib/library.svelte';
  import { t } from '$lib/i18n.svelte';
  import { ui } from './ui.svelte';
  import ToolsMenu from './ToolsMenu.svelte';

  const FORMATS: { value: ExportFormat; label: string; hint: 'ed.format.mp4' | 'ed.format.mov' }[] = [
    { value: 'mp4', label: 'MP4', hint: 'ed.format.mp4' },
    { value: 'mov', label: 'MOV', hint: 'ed.format.mov' }
  ];

  let menuOpen = $state(false);
  let exportRoot = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (exportRoot && !exportRoot.contains(e.target as Node)) menuOpen = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        menuOpen = false;
      }
    };
    window.addEventListener('mousedown', onDown, true);
    window.addEventListener('keydown', onKey, true);
    return () => {
      window.removeEventListener('mousedown', onDown, true);
      window.removeEventListener('keydown', onKey, true);
    };
  });

  async function onExport(format: ExportFormat) {
    menuOpen = false;
    try {
      if (await exportClip(format)) await refreshLibrary();
    } catch (e) {
      if (String(e).includes(CANCELLED)) {
        ui.setNotice(t('ed.exportCancelled'), 3000);
        return;
      }
      ui.setNotice(t('ed.exportError', { e: String(e) }), 6000);
      console.error('export', e);
    }
  }

  // La marca de agua se consulta al vuelo en vez de duplicar el estado que ya lleva
  // WatermarkToggle.
  async function onShare() {
    const clip = editorState.clip;
    if (!clip) return;
    const s = shareEdit();
    if (!s) {
      ui.setNotice(t('share.noBlocks'), 4000);
      return;
    }
    let watermark = false;
    try {
      watermark = await invoke<boolean>('get_watermark');
    } catch {
      // fuera de Tauri
    }
    openShare(clip, { segments: s.segments, mixer: s.mixer, format: s.format }, watermark, s.keptSec);
  }
</script>

<div class="out">
  <ToolsMenu />
  <div class="right">
    <WatermarkToggle />
    <button class="btn" onclick={onShare} disabled={editorState.exporting}>
      <Icon name="share" size={16} />
      {t('card.share')}
    </button>
    <div class="export" bind:this={exportRoot}>
      <button
        class="btn"
        class:open={menuOpen}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onclick={() => (menuOpen = !menuOpen)}
        disabled={editorState.exporting}
      >
        {editorState.exporting ? t('ed.exporting') : t('ed.export')}
        <Icon name="export" size={16} sw={2.2} />
      </button>
      {#if menuOpen}
        <div class="menu" role="menu" aria-label={t('ed.exportAs')} use:flip>
          <span class="menu-title">{t('ed.exportAs')}</span>
          {#each FORMATS as f (f.value)}
            <button role="menuitem" class="item" onclick={() => onExport(f.value)}>
              <span class="fmt mono">{f.label}</span>
              <span class="hint">{t(f.hint)}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .out {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    height: 60px;
    padding: 0 16px;
    background: var(--bg-0);
    border-top: 1px solid var(--line);
  }
  .right {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 38px;
    padding: 0 16px;
    font-size: 13.5px;
    font-weight: 560;
    color: var(--text-0);
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    transition: background 0.14s ease, border-color 0.14s ease;
  }
  .btn:hover:not(:disabled) {
    background: var(--bg-2);
  }
  .btn:disabled {
    opacity: 0.5;
  }
  .btn.open {
    background: var(--bg-2);
  }
  .export {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + 8px);
    min-width: 260px;
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
  .menu-title {
    padding: 6px 10px 8px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .item {
    display: grid;
    grid-template-columns: 44px 1fr;
    align-items: center;
    gap: 12px;
    padding: 9px 10px;
    font-size: 13px;
    text-align: left;
    color: var(--text-1);
    border-radius: 6px;
  }
  .item:hover,
  .item:focus-visible {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .fmt {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-0);
  }
  .hint {
    color: var(--text-2);
  }
</style>
