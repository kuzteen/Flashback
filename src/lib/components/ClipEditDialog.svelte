<script lang="ts">
  import { flip } from '$lib/flip';
  import { invoke } from '@tauri-apps/api/core';
  import { untrack } from 'svelte';
  import CoverFormDialog, { type CoverChange } from './CoverFormDialog.svelte';
  import SourceIcon from './SourceIcon.svelte';
  import { clipEdit, closeClipEdit, saveClipEdit } from '$lib/clip-edit.svelte';
  import { displaySource, type Clip } from '$lib/clips';
  import { library } from '$lib/library.svelte';
  import { t } from '$lib/i18n.svelte';

  const clips = $derived(
    clipEdit.paths.map((p) => library.clips.find((c) => c.path === p)).filter((c): c is Clip => !!c)
  );
  const open = $derived(clips.length > 0);
  const single = $derived(clips.length === 1 ? clips[0] : null);

  let dialog = $state<ReturnType<typeof CoverFormDialog> | null>(null);
  let name = $state('');
  // Lo que se ve en el campo y lo que se guardará: una pantalla se muestra traducida ("Screen 1")
  // pero se guarda con su nombre canónico, así que no pueden ser la misma variable.
  let gameText = $state('');
  let gameValue = $state('');
  let gameTouched = $state(false);
  // El valor viene de la lista, de lo detectado o de lo guardado, no de algo a medio escribir:
  // solo entonces la vista previa pide el icono normal del juego.
  let gamePicked = $state(true);
  let results = $state<string[]>([]);
  let listOpen = $state(false);
  let active = $state(-1);
  let searchSeq = 0;
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  // Juego común de la selección; con juegos distintos el campo arranca vacío ("Varios juegos").
  const commonSource = $derived.by(() => {
    const first = clips[0]?.source ?? '';
    return clips.every((c) => c.source === first) ? first : null;
  });

  // Al abrir se prepara el índice de juegos del backend (parsear la lista la primera vez): así la
  // primera búsqueda responde al instante.
  $effect(() => {
    if (open) invoke('search_games', { query: '' }).catch(() => {});
  });

  $effect(() => {
    void clipEdit.paths;
    untrack(() => {
      name = single?.title ?? '';
      const src = commonSource ?? '';
      gameValue = src;
      gameText = src ? displaySource(src) : '';
      gameTouched = false;
      gamePicked = true;
      results = [];
      listOpen = false;
      active = -1;
    });
  });

  const previewSource = $derived(gameTouched ? gameValue : (commonSource ?? ''));
  const detected = $derived(single?.detected ?? null);
  // Enlace para deshacer: solo con un clip y si el juego del campo ya no es el que detectó.
  const canRevert = $derived(
    detected !== null && gameValue.trim().toLowerCase() !== detected.trim().toLowerCase()
  );
  const exactHit = $derived(results.some((r) => r.toLowerCase() === gameText.trim().toLowerCase()));
  const customOption = $derived(gameText.trim() && !exactHit ? gameText.trim() : null);
  const options = $derived([...results, ...(customOption ? [customOption] : [])]);
  const previewLookup = $derived(gamePicked ? 'full' : exactHit ? 'search' : 'none');

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  function onGameInput(e: Event) {
    gameText = (e.currentTarget as HTMLInputElement).value;
    gameValue = gameText;
    gameTouched = true;
    gamePicked = false;
    listOpen = true;
    active = -1;
    clearTimeout(searchTimer);
    const query = gameText;
    const seq = ++searchSeq;
    if (!query.trim()) {
      results = [];
      return;
    }
    // Se espera a que deje de teclear; una respuesta vieja que llega tarde no pisa a la nueva.
    searchTimer = setTimeout(async () => {
      const hits = await invoke<string[]>('search_games', { query }).catch(() => [] as string[]);
      if (seq === searchSeq) results = hits;
    }, 50);
  }

  function pick(game: string) {
    gameText = game;
    gameValue = game;
    gameTouched = true;
    gamePicked = true;
    listOpen = false;
    active = -1;
  }

  function revert() {
    if (detected === null) return;
    gameValue = detected;
    gameText = detected ? displaySource(detected) : '';
    gameTouched = true;
    gamePicked = true;
    listOpen = false;
  }

  function onGameKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && listOpen) {
      // Solo la lista: el diálogo escucha Escape en window y se cerraría entero.
      e.stopPropagation();
      listOpen = false;
    } else if (e.key === 'ArrowDown' && options.length) {
      e.preventDefault();
      listOpen = true;
      active = (active + 1) % options.length;
    } else if (e.key === 'ArrowUp' && options.length) {
      e.preventDefault();
      listOpen = true;
      active = active <= 0 ? options.length - 1 : active - 1;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (listOpen && active >= 0) pick(options[active]);
      else dialog?.save();
    }
  }

  async function save(cover: CoverChange) {
    await saveClipEdit(clips, {
      name: single ? name : null,
      game: gameTouched ? gameValue : null,
      cover
    });
    closeClipEdit();
  }
</script>

<CoverFormDialog
  bind:this={dialog}
  {open}
  session={clipEdit.paths.join('|')}
  title={single ? t('clipEdit.title') : t('clipEdit.titleMany', { n: clips.length })}
  coverSrc={single?.coverSrc ?? null}
  saveLabel={t('clipEdit.save')}
  canSave={!single || !!name.trim()}
  onsave={save}
  oncancel={closeClipEdit}
>
  {#snippet placeholder()}
    <SourceIcon source={previewSource} size={96} lookup={previewLookup} />
  {/snippet}
  {#snippet fields()}
    <div class="game">
      <label class="field">
        <span class="notch">{t('clipEdit.game')}</span>
        <span class="game-ico"><SourceIcon source={previewSource} size={18} lookup={previewLookup} /></span>
        <input
          class="game-input"
          style:padding-left="40px"
          use:focusSelect
          value={gameText}
          placeholder={commonSource === null && !gameTouched ? t('clipEdit.mixed') : t('clipEdit.gamePlaceholder')}
          role="combobox"
          aria-expanded={listOpen && options.length > 0}
          aria-controls="game-results"
          aria-autocomplete="list"
          oninput={onGameInput}
          onkeydown={onGameKey}
          onfocus={() => (listOpen = options.length > 0)}
          onblur={() => setTimeout(() => (listOpen = false), 120)}
        />
      </label>
      {#if listOpen && options.length}
        <div class="results" id="game-results" role="listbox" use:flip>
          {#each options as option, i (option + i)}
            <button
              type="button"
              role="option"
              aria-selected={i === active}
              class="result"
              class:active={i === active}
              onmousedown={(e) => e.preventDefault()}
              onclick={() => pick(option)}
            >
              <SourceIcon source={option} size={20} lookup={i === results.length ? 'none' : 'search'} />
              <span class="result-name">
                {i === results.length ? t('clipEdit.useCustom', { name: option }) : option}
              </span>
            </button>
          {/each}
        </div>
      {/if}
      {#if canRevert}
        <button type="button" class="revert" onclick={revert}>
          {t('clipEdit.revert')}
        </button>
      {/if}
    </div>
    {#if single}
      <label class="field">
        <span class="notch">{t('clipEdit.name')}</span>
        <input
          bind:value={name}
          maxlength="120"
          placeholder={t('clipEdit.name')}
          onkeydown={(e) => e.key === 'Enter' && dialog?.save()}
        />
      </label>
    {/if}
  {/snippet}
</CoverFormDialog>

<style>
  .game {
    position: relative;
  }
  .game-ico {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: grid;
    color: var(--text-2);
    pointer-events: none;
  }
  .results {
    position: absolute;
    top: calc(40px + 6px);
    left: 0;
    right: 0;
    max-height: 264px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    z-index: 10;
  }
  .result {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    font-size: 13px;
    text-align: left;
    color: var(--text-1);
    border-radius: 6px;
  }
  .result:hover,
  .result.active {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .result-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Encajado en el borde superior derecho del campo, frente a la etiqueta: fuera del flujo, así
     aparecer o desaparecer no mueve el campo del nombre. Mismo fondo partido que la etiqueta
     (diálogo arriba, campo abajo) para cortar el borde. */
  .revert {
    position: absolute;
    right: 9px;
    top: 0;
    transform: translateY(-50%);
    padding: 0 4px;
    font-size: 11px;
    font-weight: 600;
    line-height: 1.2;
    color: var(--text-2);
    background: linear-gradient(to bottom, var(--bg-1) 50%, var(--bg-0) 50%);
    transition: color 0.14s ease;
  }
  .revert:hover {
    color: var(--text-0);
  }
</style>
