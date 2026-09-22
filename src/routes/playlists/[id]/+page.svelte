<script lang="ts">
  import { untrack } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import Icon from '$lib/components/Icon.svelte';
  import ClipCard from '$lib/components/ClipCard.svelte';
  import { formatDuration } from '$lib/clips';
  import { library, refreshLibrary } from '$lib/library.svelte';
  import { playlists, refreshPlaylists, findPlaylist, playlistClips, removeFromPlaylist } from '$lib/playlists.svelte';
  import { clipOrder, editorState } from '$lib/editor.svelte';
  import { selected, clearSelection, pruneSelection } from '$lib/selection.svelte';
  import { shareState } from '$lib/share.svelte';
  import { confirmState } from '$lib/confirm.svelte';
  import { t } from '$lib/i18n.svelte';
  import { cubicOut } from 'svelte/easing';

  function selbarPop(_node: Element, { duration = 200 } = {}) {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    return {
      duration: reduce ? 0 : duration,
      easing: cubicOut,
      css: (t: number, u: number) =>
        `opacity: ${t}; transform: translateX(-50%) translateY(${u * 18}px); will-change: transform, opacity;`
    };
  }

  const id = $derived(page.params.id ?? '');
  const playlist = $derived(playlists.loaded ? findPlaylist(id) : undefined);
  const clips = $derived(playlist ? playlistClips(playlist) : []);
  const total = $derived(clips.reduce((n, c) => n + c.durationSec, 0));

  $effect(() => {
    refreshPlaylists();
    if (!library.loaded) refreshLibrary();
  });

  // Al entrar y al salir: la selección es global y arrastrarla entre la biblioteca y una
  // playlist haría que la barra contara clips que no están a la vista.
  $effect(() => {
    id;
    untrack(() => clearSelection());
    return () => clearSelection();
  });

  $effect(() => {
    clipOrder.list = clips;
  });

  $effect(() => {
    const list = library.clips;
    untrack(() => pruneSelection(list));
  });

  function onKey(e: KeyboardEvent) {
    if (editorState.clip || shareState.clip || confirmState.req) return;
    const el = e.target as HTMLElement | null;
    if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) return;
    if (selected.size === 0) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      clearSelection();
    } else if (e.key === 'Delete') {
      e.preventDefault();
      removeSelected();
    }
  }

  async function removeSelected() {
    if (!playlist || selected.size === 0) return;
    const paths = clips.filter((c) => selected.has(c.id)).map((c) => c.path);
    await removeFromPlaylist(playlist.id, paths);
    clearSelection();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="page">
  <header class="head">
    <button class="back" onclick={() => goto('/playlists')} aria-label={t('pl.back')}>
      <Icon name="chevron-down" size={18} sw={2.2} />
    </button>
    {#if playlist}
      <div class="cover">
        {#if playlist.coverSrc}
          <img src={playlist.coverSrc} alt="" draggable="false" />
        {:else}
          <Icon name="heart-fill" size={24} />
        {/if}
      </div>
    {/if}
    <div class="titles">
      <h1>{playlist?.name ?? t('pl.missing')}</h1>
      {#if playlist}
        <span class="sub mono">
          {t(clips.length === 1 ? 'pl.oneClip' : 'pl.nClips', { n: clips.length })}
          {#if total > 0}<span class="dot">·</span>{formatDuration(total)}{/if}
        </span>
        {#if playlist.description}<p class="desc">{playlist.description}</p>{/if}
      {/if}
    </div>
  </header>

  {#if playlists.loaded && !playlist}
    <div class="empty">
      <Icon name="folder-fill" size={50} />
      <p>{t('pl.missing')}</p>
    </div>
  {:else if clips.length === 0}
    <div class="empty">
      <Icon name="folder-fill" size={50} />
      <p>{t('pl.emptyClips')}</p>
      <span class="hint mono">{t('pl.emptyClipsHint')}</span>
    </div>
  {:else}
    <div class="grid">
      {#each clips as clip (clip.id)}
        <ClipCard {clip} />
      {/each}
    </div>
  {/if}
</div>

{#if selected.size > 0}
  <div
    class="selbar"
    role="toolbar"
    aria-label={t('sel.bar')}
    in:selbarPop={{ duration: 220 }}
    out:selbarPop={{ duration: 140 }}
  >
    <span class="selcount mono">{t('sel.count', { n: String(selected.size) })}</span>
    <button class="selbtn" onclick={clearSelection}>{t('sel.cancel')}</button>
    <button class="selbtn danger" onclick={removeSelected}>
      <Icon name="minus" size={15} sw={2.6} />
      {t('pl.removeFrom')}
    </button>
  </div>
{/if}

<style>
  .page {
    padding: 22px 26px 40px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 26px;
  }
  .back {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    flex: none;
    color: var(--text-2);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: color 0.15s ease, border-color 0.15s ease;
  }
  .back:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  /* El chevron del set apunta hacia abajo; girarlo evita meter un icono más solo para esto. */
  .back :global(svg) {
    transform: rotate(90deg);
  }
  .cover {
    flex: none;
    width: 52px;
    height: 52px;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 9px;
    color: var(--text-3);
    background: var(--bg-2);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .titles {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .desc {
    margin-top: 4px;
    max-width: 62ch;
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-2);
  }
  h1 {
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .dot {
    margin: 0 6px;
    color: var(--text-3);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 20px;
  }
  @media (min-width: 1500px) {
    .grid {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 90px 0;
    color: var(--text-3);
  }
  .empty p {
    font-size: 14px;
    color: var(--text-2);
  }
  .empty .hint {
    font-size: 11.5px;
    color: var(--text-3);
  }

  .selbar {
    position: fixed;
    bottom: 22px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 40;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px 9px 14px;
    border-radius: 12px;
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    box-shadow: 0 18px 44px -14px rgba(0, 0, 0, 0.75);
  }
  .selcount {
    font-size: 12.5px;
    color: var(--text-1);
    margin-right: 4px;
  }
  .selbtn {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 12px;
    font-size: 13px;
    color: var(--text-1);
    border-radius: 8px;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .selbtn:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .selbtn.danger {
    color: var(--rec);
  }
  .selbtn.danger:hover {
    background: rgba(255, 91, 91, 0.12);
  }
</style>
