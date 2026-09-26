<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Tween } from 'svelte/motion';
  import { cubicOut } from 'svelte/easing';
  import Icon from '$lib/components/Icon.svelte';
  import SettingGroup from '$lib/components/settings/SettingGroup.svelte';
  import SettingRow from '$lib/components/settings/SettingRow.svelte';
  import { t } from '$lib/i18n.svelte';
  import { formatSize } from '$lib/clips';
  import { refreshLibrary, forgetThumbs } from '$lib/library.svelte';
  import { forgetArtwork } from '$lib/artwork.svelte';

  type Kind = 'editorAudio' | 'share' | 'thumbnails' | 'artwork' | 'webview';
  type Usage = { kind: Kind; bytes: number };

  const KIND_LABEL: Record<Kind, string> = {
    editorAudio: 'settings.cache.editorAudio',
    share: 'settings.cache.share',
    thumbnails: 'settings.cache.thumbnails',
    artwork: 'settings.cache.artwork',
    webview: 'settings.cache.webview'
  };

  let folder = $state('');
  let changingFolder = $state(false);
  invoke<string>('clips_dir')
    .then((d) => (folder = d))
    .catch(() => {});

  async function changeFolder() {
    if (changingFolder) return;
    changingFolder = true;
    try {
      const picked = await invoke<string | null>('pick_folder');
      if (picked) {
        await invoke('set_clips_dir', { dir: picked });
        folder = picked;
        // La biblioteca lee de la carpeta activa: recargar para reflejar el cambio.
        await refreshLibrary();
      }
    } catch (e) {
      console.error('set_clips_dir', e);
    } finally {
      changingFolder = false;
    }
  }

  function openFolder() {
    invoke('open_clips_dir').catch((e) => console.error('open_clips_dir', e));
  }

  let usage = $state<Usage[] | null>(null);
  let clearing = $state(false);
  let freed = $state<number | null>(null);
  let failed = $state(false);
  let freedTimer: ReturnType<typeof setTimeout> | undefined;
  const total = $derived(usage?.reduce((n, u) => n + u.bytes, 0) ?? 0);
  // El total cuenta hacia abajo al limpiar, al ritmo con que encogen los tramos de la barra.
  const shownTotal = new Tween(0, { duration: 700, easing: cubicOut });
  $effect(() => {
    shownTotal.target = total;
  });
  let hovered = $state<Kind | null>(null);

  function size(bytes: number) {
    return bytes < 1024 * 1024 ? '0 MB' : formatSize(bytes);
  }

  async function loadUsage() {
    usage = await invoke<Usage[]>('cache_usage').catch(() => []);
  }
  loadUsage();

  async function clear() {
    if (clearing) return;
    clearing = true;
    failed = false;
    clearTimeout(freedTimer);
    const before = total;
    try {
      await invoke('clear_cache');
    } catch (e) {
      console.error('clear_cache', e);
      failed = true;
    }
    forgetThumbs();
    forgetArtwork();
    await loadUsage();
    freed = Math.max(0, before - total);
    clearing = false;
    freedTimer = setTimeout(() => (freed = null), 4000);
  }

  $effect(() => () => clearTimeout(freedTimer));
</script>

<SettingGroup title={t('settings.group.clips')}>
  <SettingRow title={t('settings.clipsFolder')} desc={t('settings.clipsFolder.desc')}>
    {#snippet info()}<p class="path mono" title={folder}>{folder}</p>{/snippet}
    <button class="btn" onclick={openFolder}><Icon name="folder-open" size={16} sw={2} /><span class="txt">{t('settings.open')}</span></button>
    <button class="btn" onclick={changeFolder} disabled={changingFolder}><span class="txt">{t('settings.change')}</span></button>
  </SettingRow>
</SettingGroup>

<SettingGroup title={t('settings.group.cache')}>
  <SettingRow title={t('settings.cache')} desc={t('settings.cache.desc')}>
    <span class="total mono">{usage ? size(shownTotal.current) : '—'}</span>
    <button class="btn clear" class:busy={clearing || freed !== null} onclick={clear} disabled={clearing || freed !== null || !usage || total === 0}>
      {#if clearing}
        <span class="txt">{t('settings.cache.clearing')}</span>
      {:else if freed !== null}
        <Icon name="check" size={14} sw={2.4} />
        <span class="txt">{failed ? t('settings.cache.partial') : t('settings.cache.freed', { size: size(freed) })}</span>
      {:else}
        <Icon name="trash" size={15} sw={2} /><span class="txt">{t('settings.cache.clear')}</span>
      {/if}
    </button>
  </SettingRow>
  {#if usage}
    <div class="meter">
      <!-- Cada tramo crece en proporción a su tamaño (flex-grow en bytes), así encoge animado al
           limpiar sin calcular porcentajes. -->
      <div class="bar" role="presentation" onpointerleave={() => (hovered = null)}>
        {#each usage as u, i (u.kind)}
          <span
            class="slice s{i}"
            role="presentation"
            class:dim={hovered !== null && hovered !== u.kind}
            class:empty={u.bytes === 0}
            style:flex-grow={u.bytes}
            onpointerenter={() => (hovered = u.kind)}
          ></span>
        {/each}
      </div>
      <ul class="legend">
        {#each usage as u, i (u.kind)}
          <li
            class:dim={hovered !== null && hovered !== u.kind}
            onpointerenter={() => (hovered = u.kind)}
            onpointerleave={() => (hovered = null)}
          >
            <span class="dot s{i}"></span>
            <span class="name">{t(KIND_LABEL[u.kind])}</span>
            <span class="mono">{size(u.bytes)}</span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
</SettingGroup>

<style>
  .path {
    max-width: 380px;
    margin-bottom: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    color: var(--text-1);
  }

  .btn:disabled:not(.busy) {
    opacity: 0.5;
  }
  .total {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .clear {
    min-width: 168px;
  }

  .total {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-0);
  }

  .meter {
    padding: 14px 0 16px;
    border-top: 1px solid var(--line);
  }
  .bar {
    display: flex;
    gap: 2px;
    height: 8px;
    border-radius: 4px;
    overflow: hidden;
    background: var(--bg-2);
  }
  .slice {
    flex-basis: 0;
    min-width: 3px;
    transition: flex-grow 0.7s cubic-bezier(0.215, 0.61, 0.355, 1), opacity 0.15s ease;
  }
  .slice.empty {
    min-width: 0;
  }
  .s0 { background: var(--text-0); }
  .s1 { background: var(--text-1); }
  .s2 { background: var(--text-2); }
  .s3 { background: var(--text-3); }
  .s4 { background: color-mix(in srgb, var(--text-3) 55%, transparent); }
  .legend {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 0;
    font-size: 12.5px;
    color: var(--text-2);
    transition: opacity 0.15s ease;
  }
  .legend .name {
    flex: 1;
  }
  .legend .mono {
    color: var(--text-1);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex-shrink: 0;
  }
  .dim {
    opacity: 0.35;
  }
</style>
