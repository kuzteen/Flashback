<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '../Icon.svelte';
  import WatermarkToggle from '../WatermarkToggle.svelte';
  import { editorState, exportClip, shareEdit } from '$lib/editor-state.svelte';
  import { openShare } from '$lib/share.svelte';
  import { refreshLibrary } from '$lib/library.svelte';
  import { t } from '$lib/i18n.svelte';
  import { ui } from './ui.svelte';
  import ToolsMenu from './ToolsMenu.svelte';

  async function onExport() {
    try {
      const dst = await exportClip();
      if (dst) {
        await refreshLibrary();
        ui.setNotice(t('ed.exported', { name: dst.split(/[/\\]/).pop() ?? '' }), 4000);
      }
    } catch (e) {
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
    openShare(clip, { segments: s.segments, mixer: s.mixer }, watermark, s.keptSec);
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
    <button class="btn" onclick={onExport} disabled={editorState.exporting}>
      {editorState.exporting ? t('ed.exporting') : t('ed.export')}
      <Icon name="export" size={16} sw={2.2} />
    </button>
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
</style>
