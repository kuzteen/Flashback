<script lang="ts">
  import { untrack } from 'svelte';
  import Icon from '$lib/components/Icon.svelte';
  import ClipCard from '$lib/components/ClipCard.svelte';
  import LibraryFilter from '$lib/components/LibraryFilter.svelte';
  import PlaylistPicker from '$lib/components/PlaylistPicker.svelte';
  import { sortClips, clipMatchesFilters, displaySource, type LibraryFilter as Filter } from '$lib/clips';
  import { library, refreshLibrary } from '$lib/library.svelte';
  import { refreshPlaylists } from '$lib/playlists.svelte';
  import { clipOrder, editorState } from '$lib/editor.svelte';
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
  let sortAsc = $state(false);
  let sortOpen = $state(false);
  let sortEl = $state<HTMLElement | null>(null);
  let searchEl = $state<HTMLInputElement | null>(null);
  let searchFocused = $state(false);

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
      searchEl?.focus();
      searchEl?.select();
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

  const filtered = $derived(
    library.clips.filter((c) => {
      const q = query.trim().toLowerCase();
      const matchesQuery =
        !q ||
        c.title.toLowerCase().includes(q) ||
        c.source.toLowerCase().includes(q) ||
        displaySource(c.source).toLowerCase().includes(q);
      return matchesQuery && clipMatchesFilters(c, filters);
    })
  );
  const sorted = $derived(sortClips(filtered, sortAsc));

  // Por pertenencia, no por tamaño: con un filtro activo la selección puede ser mayor que lo
  // visible sin contener un solo clip de la lista.
  const allSelected = $derived(sorted.length > 0 && sorted.every((c) => selected.has(c.id)));

  function toggleAll() {
    if (allSelected) clearSelection();
    else selectAll();
  }

  // Virtualización: solo se montan las filas visibles más un colchón. El hueco de las filas
  // que faltan va como padding del contenedor, no como divs espaciadores, porque en una
  // display:grid un div de relleno ocuparía celda y descuadraría las columnas.
  const BUFFER_ROWS = 2;
  const INITIAL = 24;

  let gridEl = $state<HTMLElement | null>(null);
  let rowH = $state(0);
  let cols = $state(2);
  let start = $state(0);
  let end = $state(INITIAL);
  let scroller: HTMLElement | null = null;

  const visible = $derived(sorted.slice(start, end));
  const totalRows = $derived(Math.ceil(sorted.length / cols));
  const padTop = $derived(Math.floor(start / cols) * rowH);
  const padBottom = $derived(Math.max(0, totalRows - Math.ceil(end / cols)) * rowH);

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

  function findScroller(el: HTMLElement): HTMLElement {
    let p = el.parentElement;
    while (p) {
      const oy = getComputedStyle(p).overflowY;
      if (oy === 'auto' || oy === 'scroll') return p;
      p = p.parentElement;
    }
    return document.documentElement;
  }

  // Las columnas y la altura de fila se leen del DOM en vez de asumirlas: gridTemplateColumns
  // llega ya resuelto a anchos, así que el cálculo sobrevive a cualquier breakpoint nuevo.
  function measure() {
    if (!gridEl) return;
    const cs = getComputedStyle(gridEl);
    cols = cs.gridTemplateColumns.split(' ').filter(Boolean).length || 1;
    const card = gridEl.querySelector('.card');
    if (card) rowH = (card as HTMLElement).offsetHeight + (parseFloat(cs.rowGap) || 0);
  }

  function update() {
    if (!gridEl || !scroller || rowH <= 0) return;
    const gridTop =
      gridEl.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top +
      scroller.scrollTop;
    const into = scroller.scrollTop - gridTop;
    const rowsFit = Math.ceil(scroller.clientHeight / rowH) + BUFFER_ROWS * 2 + 1;
    const maxStart = Math.max(0, (Math.ceil(sorted.length / cols) - 1) * cols);
    const nextStart = Math.min(maxStart, Math.max(0, Math.floor(into / rowH) - BUFFER_ROWS) * cols);
    const nextEnd = Math.min(sorted.length, nextStart + rowsFit * cols);
    if (nextStart !== start) start = nextStart;
    if (nextEnd !== end) end = nextEnd;
  }

  $effect(() => {
    const el = gridEl;
    if (!el) return;
    scroller = findScroller(el);
    untrack(() => {
      measure();
      update();
    });
    let ticking = false;
    const onScroll = () => {
      if (ticking) return;
      ticking = true;
      requestAnimationFrame(() => {
        ticking = false;
        update();
      });
    };
    scroller.addEventListener('scroll', onScroll, { passive: true });
    // Solo se observa el scroller: la rejilla cambia de alto cada vez que ajustamos su
    // padding, y observarla realimentaría el propio cálculo.
    const ro = new ResizeObserver(() => {
      measure();
      update();
    });
    ro.observe(scroller);
    const sc = scroller;
    return () => {
      sc.removeEventListener('scroll', onScroll);
      ro.disconnect();
    };
  });

  $effect(() => {
    sorted.length;
    untrack(() => {
      measure();
      update();
    });
  });

  $effect(() => {
    if (!sortOpen) return;
    const onDown = (e: MouseEvent) => {
      if (sortEl && !sortEl.contains(e.target as Node)) sortOpen = false;
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  });

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

    <div class="right">
      <label class="search" class:open={searchFocused || query}>
        <Icon name="search" size={16} />
        <input
          aria-label={t('clips.search')}
          placeholder={t('clips.search')}
          bind:this={searchEl}
          bind:value={query}
          onfocus={() => (searchFocused = true)}
          onblur={() => (searchFocused = false)}
          onkeydown={(e) => e.key === 'Escape' && searchEl?.blur()}
        />
      </label>
      <LibraryFilter clips={library.clips} bind:selected={filters} />
      <div class="sort-dd" class:open={sortOpen} bind:this={sortEl}>
        <button class="ctrl" onclick={() => (sortOpen = !sortOpen)}>
          <Icon name="sort" size={14} />
          {sortAsc ? t('clips.oldest') : t('clips.newest')}
          <Icon name="chevron-down" size={13} sw={2} />
        </button>
        {#if sortOpen}
          <div class="sort-menu">
            <button class="sort-item" class:on={!sortAsc} onclick={() => { sortAsc = false; sortOpen = false; }}>
              {t('clips.newest')}
            </button>
            <button class="sort-item" class:on={sortAsc} onclick={() => { sortAsc = true; sortOpen = false; }}>
              {t('clips.oldest')}
            </button>
          </div>
        {/if}
      </div>
    </div>
  </header>

  {#if library.clips.length === 0}
    <div class="empty">
      <Icon name="clips" size={50} sw={1.3} />
      <p>{t('clips.emptyNone')}</p>
      <span class="hint mono">{t('clips.emptyNoneHint')}</span>
    </div>
  {:else if filtered.length === 0}
    <div class="empty">
      <Icon name="chevrons" size={56} sw={1.2} />
      <p>{query ? t('clips.noResultsQuery', { query }) : t('clips.noResultsFilter')}</p>
    </div>
  {:else}
    <div
      class="grid"
      bind:this={gridEl}
      style:padding-top="{padTop}px"
      style:padding-bottom="{padBottom}px"
    >
      {#each visible as clip (clip.id)}
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
    <button
      class="selall"
      class:all={allSelected}
      role="checkbox"
      aria-checked={allSelected ? 'true' : 'mixed'}
      aria-label={t('sel.all')}
      title={t('sel.all')}
      onclick={toggleAll}
    >
      <Icon name={allSelected ? 'check' : 'minus'} size={14} sw={2.8} />
    </button>
    <span class="selcount mono">{t('sel.count', { n: String(selected.size) })}</span>
    <div class="pl-dd" bind:this={plEl}>
      <button class="selbtn" class:on={plOpen} onclick={() => (plOpen = !plOpen)}>
        <Icon name="folder-plus" size={14} />
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
      <Icon name="trash" size={15} sw={1.9} />
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
.right {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 9px;
    height: 36px;
    width: 36px;
    overflow: hidden;
    cursor: pointer;
    color: var(--text-1);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: width 0.22s cubic-bezier(0.2, 0.8, 0.2, 1), border-color 0.15s ease, color 0.15s ease;
  }
  .search :global(svg) {
    flex-shrink: 0;
  }
  .search:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .search.open {
    width: 220px;
    cursor: text;
  }
  .search:focus-within {
    border-color: var(--line-strong);
  }
  .search input {
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    outline: none;
    font-size: 13px;
    color: var(--text-0);
  }
  .search input::placeholder {
    color: var(--text-3);
  }
  .ctrl {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 12px;
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
  .sort-dd {
    position: relative;
  }
  .sort-dd.open .ctrl {
    border-color: var(--line-strong);
    color: var(--text-0);
  }
  .sort-dd .ctrl > :global(svg:last-child) {
    transition: transform 0.2s ease;
  }
  .sort-dd.open .ctrl > :global(svg:last-child) {
    transform: rotate(180deg);
  }
  .sort-menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    min-width: 150px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    box-shadow: 0 18px 42px -14px rgba(0, 0, 0, 0.7);
    z-index: 70;
  }
  .sort-item {
    padding: 7px 10px;
    font-size: 13px;
    text-align: left;
    color: var(--text-1);
    border-radius: 6px;
    transition: background 0.13s ease, color 0.13s ease;
  }
  .sort-item:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .sort-item.on {
    color: var(--text-0);
    font-weight: 560;
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
    padding: 9px 10px 9px 11px;
    border-radius: 12px;
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    box-shadow: 0 18px 44px -14px rgba(0, 0, 0, 0.75);
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
    box-shadow: 0 18px 42px -14px rgba(0, 0, 0, 0.7);
  }
  .selbtn.danger {
    color: var(--rec);
  }
  .selbtn.danger:hover {
    background: rgba(255, 91, 91, 0.12);
  }
  .selbtn:disabled {
    opacity: 0.5;
  }
</style>
