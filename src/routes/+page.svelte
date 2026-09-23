<script lang="ts">
  import { untrack } from 'svelte';
  import Icon from '$lib/components/Icon.svelte';
  import SortableGrid from '$lib/components/SortableGrid.svelte';
  import ClipCard from '$lib/components/ClipCard.svelte';
  import ClipToolbar from '$lib/components/ClipToolbar.svelte';
  import PlaylistPicker from '$lib/components/PlaylistPicker.svelte';
  import { sortClips, clipMatchesFilters, displaySource, type LibraryFilter as Filter } from '$lib/clips';
  import { library, refreshLibrary, clipView } from '$lib/library.svelte';
  import { refreshPlaylists } from '$lib/playlists.svelte';
  import { clipOrder, editorState } from '$lib/editor-state.svelte';
  import { selected, clearSelection, selectAll, pruneSelection } from '$lib/selection.svelte';
  import { confirmDelete, confirmState } from '$lib/confirm.svelte';
  import { shareState } from '$lib/share.svelte';
  import { removeFavorite } from '$lib/library.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '$lib/i18n.svelte';
  import { cubicOut } from 'svelte/easing';

  // La barra se centra con translateX(-50%), asi que la transicion tiene que reescribir el
  // transform completo: si solo emitiera translateY, perderia el centrado a mitad de animacion.
  // Sin scale a proposito: escalar obliga a rasterizar de nuevo el contenido en cada fotograma y
  // el trazo del icono de la papelera bailaba. will-change va inline, solo mientras dura.
  function selbarPop(_node: Element, { duration = 200 } = {}) {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    return {
      duration: reduce ? 0 : duration,
      easing: cubicOut,
      css: (t: number, u: number) =>
        `opacity: ${t}; transform: translateX(-50%) translateY(${u * 18}px); will-change: transform, opacity;`
    };
  }

  let query = $state('');
  let filters = $state<Filter[]>([]);
  let sort = $state<'newest' | 'oldest'>('newest');
  let toolbar = $state<ClipToolbar<'newest' | 'oldest'> | null>(null);
  const sorts = [
    { value: 'newest' as const, label: 'clips.newest' },
    { value: 'oldest' as const, label: 'clips.oldest' }
  ];

  function onKey(e: KeyboardEvent) {
    // Con un modal delante las teclas son suyas: Supr abriría una segunda confirmación encima de
    // la primera, y Escape cerraría los dos a la vez.
    if (editorState.clip || shareState.clip || confirmState.req) return;
    const el = e.target as HTMLElement | null;
    const typing =
      !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable);
    const ctrl = e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey;

    if (ctrl && e.key.toLowerCase() === 's') {
      e.preventDefault();
      toolbar?.focusSearch();
      return;
    }
    if (typing) return;
    if (ctrl && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      selectAll();
      return;
    }
    if (selected.size === 0) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      clearSelection();
    } else if (e.key === 'Delete') {
      e.preventDefault();
      deleteSelected(e);
    }
  }

  $effect(() => {
    refreshLibrary();
    // La pertenencia a playlists se marca en el menú de cada tarjeta, así que el índice tiene
    // que estar cargado antes de abrirlo.
    refreshPlaylists();
  });

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return library.clips.filter((c) => {
      const matchesQuery =
        !q ||
        c.title.toLowerCase().includes(q) ||
        c.source.toLowerCase().includes(q) ||
        displaySource(c.source).toLowerCase().includes(q);
      return matchesQuery && clipMatchesFilters(c, filters);
    });
  });
  const sorted = $derived(sortClips(filtered, sort === 'oldest'));

  // Por pertenencia, no por tamaño: con un filtro activo la selección puede ser mayor que lo
  // visible sin contener un solo clip de la lista.
  const allSelected = $derived(sorted.length > 0 && sorted.every((c) => selected.has(c.id)));

  function toggleAll() {
    if (allSelected) clearSelection();
    else selectAll();
  }

  let deleting = $state(false);
  let plOpen = $state(false);
  let plEl = $state<HTMLElement | null>(null);
  const selectedPaths = $derived(library.clips.filter((c) => selected.has(c.id)).map((c) => c.path));

  $effect(() => {
    if (selected.size === 0) plOpen = false;
  });

  $effect(() => {
    if (!plOpen) return;
    const onDown = (e: MouseEvent) => {
      if (plEl && !plEl.contains(e.target as Node)) plOpen = false;
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  });

  async function deleteSelected(e: { shiftKey: boolean }) {
    if (deleting || selected.size === 0) return;
    const ids = [...selected];
    const first = library.clips.find((c) => c.id === ids[0]);
    // Shift salta la confirmación: el borrado va a la papelera, así que el atajo no es
    // irreversible para quien ya sabe lo que hace.
    if (!e.shiftKey && !(await confirmDelete(ids.length, first?.title))) return;
    deleting = true;
    // Sobre la biblioteca completa, no sobre lo filtrado: si el usuario marca clips y luego
    // escribe en el buscador, el contador seguiría diciendo 5 pero solo se borrarían los
    // visibles.
    const paths = library.clips.filter((c) => selected.has(c.id)).map((c) => c.path);
    try {
      await invoke('delete_clips', { paths });
      for (const id of ids) removeFavorite(id);
      clearSelection();
      await refreshLibrary();
    } catch (err) {
      console.error('delete_clips', err);
    } finally {
      deleting = false;
    }
  }

  // El editor navega anterior/siguiente por este mismo orden.
  $effect(() => {
    clipOrder.list = sorted;
  });

  // Contra la biblioteca entera, no contra lo filtrado: buscar no debe deshacer una selección.
  $effect(() => {
    const clips = library.clips;
    untrack(() => pruneSelection(clips));
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="clips">
  <header class="head">
    <div class="left">
      <h1>{t('clips.title')}</h1>
    </div>

    <ClipToolbar clips={library.clips} bind:query bind:filters bind:sort {sorts} bind:this={toolbar} />
  </header>

  {#if library.clips.length === 0}
    <div class="empty">
      <Icon name="clips" size={50} sw={1.3} />
      <p>{t('clips.emptyNone')}</p>
      <span class="hint mono">{t('clips.emptyNoneHint')}</span>
    </div>
  {:else if filtered.length === 0}
    <div class="empty">
      <span class="logo-mark"></span>
      <p>{query ? t('clips.noResultsQuery', { query }) : t('clips.noResultsFilter')}</p>
    </div>
  {:else}
    <!-- Remontar al cambiar de vista: la virtualización mide el alto de fila al montar, y con
         la rejilla viva se quedaría con el de la vista anterior. -->
    {#key clipView.mode}
      <SortableGrid items={sorted} key={(c) => c.id}>
        {#snippet children(clip)}<ClipCard {clip} playlistTag compact={clipView.mode === 'list'} />{/snippet}
      </SortableGrid>
    {/key}
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
    <button
      class="selall"
      class:all={allSelected}
      role="checkbox"
      aria-checked={allSelected ? 'true' : 'mixed'}
      aria-label={t('sel.all')}
      onclick={toggleAll}
    >
      <Icon name={allSelected ? 'check' : 'minus'} size={14} sw={2.8} />
    </button>
    <span class="selcount mono">{t('sel.count', { n: String(selected.size) })}</span>
    <div class="pl-dd" bind:this={plEl}>
      <button class="selbtn" class:on={plOpen} onclick={() => (plOpen = !plOpen)}>
        <Icon name="folder-plus" size={16} />
        {t('pl.addTo')}
      </button>
      {#if plOpen}
        <div class="pl-menu">
          <PlaylistPicker paths={selectedPaths} onclose={() => (plOpen = false)} />
        </div>
      {/if}
    </div>
    <button class="selbtn" onclick={clearSelection}>{t('sel.cancel')}</button>
    <button class="selbtn danger" disabled={deleting} onclick={deleteSelected}>
      <Icon name="trash" size={16} sw={2} />
      {t('sel.delete')}
    </button>
  </div>
{/if}

<style>
  .clips {
    padding: 22px 26px 40px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    margin-bottom: 26px;
    flex-wrap: wrap;
  }
  .left {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  h1 {
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 90px 0;
    color: var(--text-3);
  }
  /* El logo como máscara, para que tome el color apagado del aviso. */
  .logo-mark {
    width: 46px;
    height: 46px;
    background-color: currentColor;
    -webkit-mask: url('/flashback-mono.svg') center / contain no-repeat;
    mask: url('/flashback-mono.svg') center / contain no-repeat;
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
    padding: 9px 10px 9px 11px;
    border-radius: 12px;
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-pop);
  }
  .selall {
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    color: var(--text-2);
    border: 1px solid var(--line-strong);
    transition: background 0.14s ease, border-color 0.14s ease, color 0.14s ease;
  }
  .selall:hover {
    color: var(--text-0);
    border-color: var(--text-3);
  }
  .selall.all {
    color: var(--base);
    background: var(--bright);
    border-color: var(--bright);
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
  .pl-dd {
    position: relative;
  }
  .selbtn.on {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .pl-menu {
    position: absolute;
    bottom: calc(100% + 10px);
    left: 50%;
    transform: translateX(-50%);
    width: 216px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
  }
  .selbtn.danger {
    color: var(--rec);
  }
  .selbtn.danger:hover {
    background: color-mix(in srgb, var(--rec) 12%, transparent);
  }
  .selbtn:disabled {
    opacity: 0.5;
  }
</style>
