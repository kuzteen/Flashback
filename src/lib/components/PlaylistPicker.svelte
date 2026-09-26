<script lang="ts">
  import { hoverPill } from '$lib/pill';
  import Icon from './Icon.svelte';
  import {
    playlists,
    refreshPlaylists,
    createPlaylist,
    addToPlaylist,
    removeFromPlaylist,
    orderedPlaylists
  } from '$lib/playlists.svelte';
  import { t } from '$lib/i18n.svelte';

  // paths: los clips sobre los que actúa (uno desde la tarjeta, varios desde la selección).
  let { paths, onclose }: { paths: string[]; onclose?: () => void } = $props();

  let creating = $state(false);
  let newName = $state('');

  $effect(() => {
    if (!playlists.loaded) refreshPlaylists();
  });

  const sorted = $derived(
    orderedPlaylists(playlists.list)
  );

  // Con varios clips marcados la fila solo se da por "dentro" cuando están todos: si no,
  // pulsarla quitaría los que ya estaban en vez de completar el grupo.
  function membership(id: string): 'in' | 'partial' | 'out' {
    const pl = playlists.list.find((p) => p.id === id);
    if (!pl) return 'out';
    const n = paths.filter((x) => pl.clips.some((c) => c.path === x)).length;
    if (n === 0) return 'out';
    return n === paths.length ? 'in' : 'partial';
  }

  async function toggle(id: string) {
    if (membership(id) === 'in') await removeFromPlaylist(id, paths);
    else await addToPlaylist(id, paths);
  }

  async function commitCreate() {
    if (!creating) return;
    const name = newName.trim();
    creating = false;
    newName = '';
    if (!name) return;
    const pl = await createPlaylist(name);
    if (pl) await addToPlaylist(pl.id, paths);
  }

  function focusInput(node: HTMLInputElement) {
    node.focus();
  }
</script>

<div class="picker" role="menu">
  <div class="rows" use:hoverPill={{ axis: 'y' }}>
    {#each sorted as p (p.id)}
      {@const st = membership(p.id)}
      <button
        role="menuitemcheckbox"
        aria-checked={st === 'in'}
        class:on={st !== 'out'}
        onclick={(e) => {
          e.stopPropagation();
          toggle(p.id);
        }}
      >
        <span class="box" class:full={st === 'in'} class:half={st === 'partial'}>
          <Icon name={st === 'partial' ? 'minus' : 'check'} size={11} sw={3} />
        </span>
        <span class="nm">{p.name}</span>
        <span class="n mono">{p.clips.length}</span>
      </button>
    {/each}
    {#if sorted.length === 0 && !creating}
      <span class="none">{t('pl.pickerEmpty')}</span>
    {/if}
  </div>

  <div class="sep"></div>

  {#if creating}
    <input
      class="new-input"
      placeholder={t('pl.namePlaceholder')}
      bind:value={newName}
      use:focusInput
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => {
        e.stopPropagation();
        if (e.key === 'Enter') commitCreate();
        else if (e.key === 'Escape') {
          creating = false;
          onclose?.();
        }
      }}
      onblur={commitCreate}
    />
  {:else}
    <button
      class="new"
      role="menuitem"
      onclick={(e) => {
        e.stopPropagation();
        newName = '';
        creating = true;
      }}
    >
      <Icon name="folder-plus" size={16} />
      {t('pl.new')}
    </button>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  /* Tope de alto: con muchas playlists el menú se convertía en una columna más alta que la
     ventana y el botón de crear quedaba fuera de alcance. */
  .rows {
    display: flex;
    flex-direction: column;
    gap: 1px;
    max-height: 220px;
    overflow-y: auto;
  }
  .picker button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    color: var(--text-1);
    text-align: left;
    border-radius: 6px;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .picker button:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .picker button.on {
    color: var(--text-0);
  }
  .rows button:hover {
    background: none;
  }
  .rows > :global(.slide-pill) {
    background: var(--bg-3);
    border-radius: 6px;
  }
  .box {
    flex: none;
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    border-radius: 4px;
    color: transparent;
    border: 1px solid var(--line-strong);
    transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
  }
  .box.full,
  .box.half {
    color: var(--base);
    background: var(--bright);
    border-color: var(--bright);
  }
  .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .n {
    flex: none;
    font-size: 11px;
    color: var(--text-3);
  }
  .none {
    padding: 8px 10px;
    font-size: 12.5px;
    color: var(--text-3);
  }
  /* 16 px como la casilla de las filas de arriba: así el texto de todas las filas arranca en
     la misma columna. */
  .new :global(svg) {
    flex-shrink: 0;
  }
  .new-input {
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--accent);
    border-radius: 6px;
    outline: none;
  }
  .new-input::placeholder {
    color: var(--text-3);
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
  }
</style>
