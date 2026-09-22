<script lang="ts">
  import { goto } from '$app/navigation';
  import Icon from '$lib/components/Icon.svelte';
  import PlaylistCard from '$lib/components/PlaylistCard.svelte';
  import SortableGrid from '$lib/components/SortableGrid.svelte';
  import { library, refreshLibrary } from '$lib/library.svelte';
  import {
    playlists,
    refreshPlaylists,
    openPlaylistCreate,
    orderedPlaylists,
    reorderPlaylists,
    type Playlist
  } from '$lib/playlists.svelte';
  import { t } from '$lib/i18n.svelte';

  $effect(() => {
    refreshPlaylists();
    if (!library.loaded) refreshLibrary();
  });

  const sorted = $derived(orderedPlaylists(playlists.list));

  // Al primer arrastre todas reciben posición, así que el orden por fecha deja de aplicar a
  // las que ya había y solo sigue poniendo arriba las que se creen después.
  function reorder(from: number, to: number) {
    const ids = sorted.map((p) => p.id);
    const [moved] = ids.splice(from, 1);
    ids.splice(to > from ? to - 1 : to, 0, moved);
    reorderPlaylists(ids);
  }

  function openPlaylist(p: Playlist) {
    goto(`/playlists/${p.id}`);
  }
</script>

<div class="page">
  <header class="head">
    <h1>{t('pl.title')}</h1>
    <button class="ctrl" onclick={openPlaylistCreate}>
      <Icon name="plus" size={15} sw={2.2} />
      {t('pl.new')}
    </button>
  </header>

  {#if sorted.length === 0}
    <div class="empty">
      <Icon name="folder-fill" size={50} />
      <p>{t('pl.emptyNone')}</p>
      <span class="hint mono">{t('pl.emptyNoneHint')}</span>
    </div>
  {:else}
    <SortableGrid items={sorted} key={(p) => p.id} onreorder={sorted.length > 1 ? reorder : undefined}>
      {#snippet children(p)}<PlaylistCard playlist={p} onopen={openPlaylist} />{/snippet}
    </SortableGrid>
  {/if}
</div>

<style>
  .page {
    padding: 22px 26px 40px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    margin-bottom: 26px;
  }
  h1 {
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .ctrl {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 13px;
    font-size: 13px;
    color: var(--text-1);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: color 0.15s ease, border-color 0.15s ease;
  }
  .ctrl:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
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
</style>
