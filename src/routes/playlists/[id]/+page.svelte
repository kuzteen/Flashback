<script lang="ts">
  import { untrack } from 'svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import Icon from '$lib/components/Icon.svelte';
  import SortableGrid from '$lib/components/SortableGrid.svelte';
  import ClipCard from '$lib/components/ClipCard.svelte';
  import ClipToolbar from '$lib/components/ClipToolbar.svelte';
  import UndoToast from '$lib/components/UndoToast.svelte';
  import {
    formatDuration,
    sortClips,
    clipMatchesFilters,
    displaySource,
    type Clip,
    type LibraryFilter as Filter
  } from '$lib/clips';
  import { library, refreshLibrary, clipView } from '$lib/library.svelte';
  import {
    playlists,
    refreshPlaylists,
    findPlaylist,
    playlistClips,
    removeWithUndo,
    playlistUndo,
    undoPlaylistRemove,
    isFreshInPlaylist,
    markPlaylistClipSeen,
    openPlaylistEdit,
    reorderPlaylist
  } from '$lib/playlists.svelte';
  import { clipOrder, editorState } from '$lib/editor-state.svelte';
  import { selected, clearSelection, selectAll, pruneSelection } from '$lib/selection.svelte';
  import { shareState } from '$lib/share.svelte';
  import { confirmState } from '$lib/confirm.svelte';
  import { t } from '$lib/i18n.svelte';
  import { TOAST_IN, TOAST_OUT, toastSlide } from '$lib/toast-motion';
  import { hoverPill } from '$lib/pill';


  const id = $derived(page.params.id ?? '');
  const playlist = $derived(playlists.loaded ? findPlaylist(id) : undefined);
  const clips = $derived(playlist ? playlistClips(playlist) : []);
  const total = $derived(clips.reduce((n, c) => n + c.durationSec, 0));

  // Por ruta, que es la clave con la que la playlist guarda fecha y marca de visto.
  const refs = $derived(new Map((playlist?.clips ?? []).map((c, i) => [c.path, { ...c, i }])));

  let query = $state('');
  let filters = $state<Filter[]>([]);
  // El orden propio va primero y por defecto: es el único que el usuario decide, y el único en
  // el que arrastrar tiene sentido.
  type SortMode = 'custom' | 'added' | 'newest' | 'oldest';
  let sortMode = $state<SortMode>('custom');
  let toolbar = $state<ClipToolbar<SortMode> | null>(null);
  const sorts: { value: SortMode; label: string }[] = [
    { value: 'custom', label: 'pl.sortCustom' },
    { value: 'added', label: 'pl.sortAdded' },
    { value: 'newest', label: 'clips.newest' },
    { value: 'oldest', label: 'clips.oldest' }
  ];

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return clips.filter((c) => {
      const matchesQuery =
        !q ||
        c.title.toLowerCase().includes(q) ||
        c.source.toLowerCase().includes(q) ||
        displaySource(c.source).toLowerCase().includes(q);
      return matchesQuery && clipMatchesFilters(c, filters);
    });
  });

  // Los clips sin fecha (añadidos antes de que se guardara) desempatan por su posición en el
  // índice, que es el único rastro que queda del orden en que entraron.
  function byAdded(list: Clip[]): Clip[] {
    return [...list].sort((a, b) => {
      const ra = refs.get(a.path);
      const rb = refs.get(b.path);
      const ta = ra?.addedAt?.getTime() ?? 0;
      const tb = rb?.addedAt?.getTime() ?? 0;
      if (ta !== tb) return tb - ta;
      return (rb?.i ?? 0) - (ra?.i ?? 0);
    });
  }

  const sorted = $derived(
    sortMode === 'newest'
      ? sortClips(filtered, false)
      : sortMode === 'oldest'
        ? sortClips(filtered, true)
        : sortMode === 'added'
          ? byAdded(filtered)
          : filtered
  );

  // Solo se arrastra sobre la lista completa en su orden propio: con un filtro o un orden
  // derivado, "soltar entre A y B" no tiene una posición única en la playlist real.
  const canReorder = $derived(
    sortMode === 'custom' && !query.trim() && filters.length === 0 && sorted.length > 1
  );

  // Los clips que la biblioteca no encuentra (unidad desconectada, carpeta quitada) no se ven
  // pero siguen en la playlist: van al final para que vuelvan si reaparecen.
  function reorder(from: number, to: number) {
    if (!playlist) return;
    const list = sorted.map((c) => c.path);
    const [moved] = list.splice(from, 1);
    list.splice(to > from ? to - 1 : to, 0, moved);
    const shown = new Set(list);
    const hidden = playlist.clips.map((c) => c.path).filter((p) => !shown.has(p));
    reorderPlaylist(playlist.id, [...list, ...hidden]);
  }

  // Abrir el clip lo da por visto y apaga su pill. Depende solo de la ruta abierta: leer la
  // playlist sin untrack reentraría en el efecto con cada escritura del índice.
  $effect(() => {
    const path = editorState.clip?.path;
    if (!path) return;
    untrack(() => {
      const p = playlist;
      if (p && p.clips.some((c) => c.path === path)) markPlaylistClipSeen(p.id, path);
    });
  });

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

  // El editor navega anterior/siguiente por este mismo orden.
  $effect(() => {
    clipOrder.list = sorted;
  });

  $effect(() => {
    const list = library.clips;
    untrack(() => pruneSelection(list));
  });

  // Por pertenencia, no por tamaño: con un filtro activo la selección puede ser mayor que lo
  // visible sin contener un solo clip de la lista.
  const allSelected = $derived(sorted.length > 0 && sorted.every((c) => selected.has(c.id)));

  function toggleAll() {
    if (allSelected) clearSelection();
    else selectAll();
  }

  // El aviso de deshacer es de esta página: al cambiar de playlist o salir, lo quitado se queda.
  $effect(() => {
    id;
    untrack(() => (playlistUndo.current = null));
    return () => (playlistUndo.current = null);
  });

  function onKey(e: KeyboardEvent) {
    if (editorState.clip || shareState.clip || confirmState.req) return;
    const el = e.target as HTMLElement | null;
    const typing =
      !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable);
    if (e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey && e.key.toLowerCase() === 's') {
      e.preventDefault();
      toolbar?.focusSearch();
      return;
    }
    if (typing) return;
    const ctrl = e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey;
    if (ctrl && e.key.toLowerCase() === 'z' && playlistUndo.current) {
      e.preventDefault();
      undoPlaylistRemove();
      return;
    }
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
      removeSelected();
    }
  }

  async function removeSelected() {
    if (!playlist || selected.size === 0) return;
    const paths = clips.filter((c) => selected.has(c.id)).map((c) => c.path);
    await removeWithUndo(playlist.id, paths);
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
      <button
        class="cover"
        aria-label={t('pl.edit')}
        onclick={() => openPlaylistEdit(playlist.id)}
      >
        {#if playlist.coverSrc}
          <img src={playlist.coverSrc} alt="" draggable="false" />
        {:else}
          <Icon name="heart-fill" size={44} />
        {/if}
        <span class="cover-hint"><Icon name="rename" size={24} sw={1.9} /></span>
      </button>
    {/if}
    <div class="titles">
      <h1>{playlist?.name ?? t('pl.missing')}</h1>
      {#if playlist}
        {#if playlist.description}<p class="desc">{playlist.description}</p>{/if}
        <span class="sub mono">
          {t(clips.length === 1 ? 'pl.oneClip' : 'pl.nClips', { n: clips.length })}
          {#if total > 0}<span class="dot">•</span>{formatDuration(total)}{/if}
        </span>
      {/if}
    </div>

    {#if playlist && clips.length > 0}
      <div class="right">
        <ClipToolbar {clips} bind:query bind:filters bind:sort={sortMode} {sorts} bind:this={toolbar} />
      </div>
    {/if}
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
  {:else if sorted.length === 0}
    <div class="empty">
      <span class="logo-mark"></span>
      <p>{query ? t('clips.noResultsQuery', { query }) : t('clips.noResultsFilter')}</p>
    </div>
  {:else}
    <!-- Remontar al cambiar de vista: la virtualización mide el alto de fila al montar, y con
         la rejilla viva se quedaría con el de la vista anterior. -->
    {#key clipView.mode}
      <SortableGrid items={sorted} key={(c) => c.id} onreorder={canReorder ? reorder : undefined}>
        {#snippet children(clip)}
          <ClipCard
            {clip}
            compact={clipView.mode === 'list'}
            fresh={isFreshInPlaylist(refs.get(clip.path))}
            playlistId={id}
          />
        {/snippet}
      </SortableGrid>
    {/key}
  {/if}
</div>

{#if selected.size > 0}
  <div
    class="selbar"
    use:hoverPill={{ selector: '.selbtn' }}
    role="toolbar"
    aria-label={t('sel.bar')}
    in:toastSlide={{ from: 1, duration: TOAST_IN }}
    out:toastSlide={{ from: 1, duration: TOAST_OUT }}
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
    <button class="selbtn" onclick={clearSelection}>{t('sel.cancel')}</button>
    <button class="selbtn danger" data-tone="danger" onclick={removeSelected}>
      <Icon name="minus" size={16} sw={2} />
      {t('pl.removeFrom')}
    </button>
  </div>
{/if}

{#if playlistUndo.current}
  {@const undo = playlistUndo.current}
  <UndoToast
    seq={undo.seq}
    title={undo.removed.length === 1
      ? t('pl.removedOne')
      : t('pl.removedN', { n: String(undo.removed.length) })}
    onundo={undoPlaylistRemove}
    ondone={() => (playlistUndo.current = null)}
  />
{/if}

<style>
  .page {
    padding: 22px 26px 40px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 28px;
    flex-wrap: wrap;
  }
  .right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
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
  /* 110 px y no más: las portadas se rasterizan a 256 px (COVER_PX), y con el escalado de
     Windows a 1.5x una portada mayor pediría más píxeles de los que tiene el PNG. */
  .cover {
    position: relative;
    flex: none;
    width: 110px;
    height: 110px;
    padding: 0;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 12px;
    color: var(--text-3);
    background: var(--bg-2);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  /* Solo al pasar por encima: la portada es lo primero que se mira de la playlist y un icono
     permanente encima competiría con ella. */
  .cover-hint {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--text-0);
    background: rgba(0, 0, 0, 0.55);
    opacity: 0;
    transition: opacity 0.15s ease;
  }
  .cover:hover .cover-hint,
  .cover:focus-visible .cover-hint {
    opacity: 1;
  }
  .titles {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  /* Dos líneas como mucho: el límite del campo son 100 caracteres, pero las playlists
     guardadas con el límite anterior traen descripciones más largas. */
  .desc {
    max-width: 70ch;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--text-2);
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }
  h1 {
    font-size: 30px;
    font-weight: 700;
    letter-spacing: -0.02em;
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
    translate: -50% 0;
    z-index: 40;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 64px;
    padding: 0 12px 0 16px;
    border-radius: var(--r-lg);
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
    color: var(--text-0);
  }
  .selbar > :global(.slide-pill) {
    background: var(--bg-3);
    border-radius: 8px;
  }
  .selbar > :global(.slide-pill[data-tone='danger']) {
    background: color-mix(in srgb, var(--rec) 12%, transparent);
  }
  .selbtn.danger {
    color: var(--rec);
  }

</style>
