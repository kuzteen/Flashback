<script lang="ts">
  import { flip } from '$lib/flip';
  import Icon from './Icon.svelte';
  import { menu } from '$lib/menu.svelte';
  import { formatDuration, isScreenSource, displaySource } from '$lib/clips';
  import { gameIcon, ensureGameIcon } from '$lib/artwork.svelte';
  import { groupCover, initial } from '$lib/source-badge';
  import { playlistClips, deletePlaylist, openPlaylistEdit, type Playlist } from '$lib/playlists.svelte';
  import { confirmDeletePlaylist } from '$lib/confirm.svelte';
  import { t } from '$lib/i18n.svelte';

  let { playlist, onopen }: { playlist: Playlist; onopen: (p: Playlist) => void } = $props();

  const clips = $derived(playlistClips(playlist));
  const total = $derived(clips.reduce((n, c) => n + c.durationSec, 0));
  const open = $derived(menu.openId === playlist.id);

  // Por número de clips, descendente. El desempate alfabético no es cosmético: sin él, dos
  // juegos empatados podrían intercambiarse de sitio al añadir un clip de un tercero.
  const games = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const c of clips) {
      if (!c.source) continue;
      counts.set(c.source, (counts.get(c.source) ?? 0) + 1);
    }
    const slots: { source: string; n: number; screen: boolean }[] = [];
    // Set y no lista: un clip puede traer ya varias pantallas en su source, y sin aplanarlas
    // el tooltip repetiría el mismo monitor.
    const screens = new Set<string>();
    let screenClips = 0;
    for (const [source, n] of counts) {
      if (isScreenSource(source)) {
        for (const part of source.split(' · ')) screens.add(part.trim());
        screenClips += n;
      } else {
        slots.push({ source, n, screen: false });
      }
    }
    // Todas las pantallas dibujan el mismo icono, así que un hueco por monitor no diría nada
    // nuevo: van en uno solo. El source compuesto es el formato que displaySource ya entiende,
    // de modo que el tooltip las nombra todas y localizadas sin tratarlas aparte.
    if (screens.size > 0) {
      const names = [...screens].sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
      slots.push({ source: names.join(' · '), n: screenClips, screen: true });
    }
    return slots.sort((a, b) => b.n - a.n || a.source.localeCompare(b.source));
  });

  // La fila nunca pasa de 4 huecos. Si no caben todos, el cuarto se atenúa y lleva el número
  // encima: cuenta los que no se ven, él incluido. Con 4 justos no sobra ninguno y se enseñan
  // los cuatro sin número, porque ahí el número no añadiría información.
  const SLOTS = 4;
  const shown = $derived(games.slice(0, SLOTS));
  const overflow = $derived(games.length > SLOTS ? games.length - (SLOTS - 1) : 0);

  // El cuarto icono, cuando lleva el +N, representa a varios: su tooltip los nombra a todos.
  const hiddenNames = $derived(
    games
      .slice(SLOTS - 1)
      .map((g) => displaySource(g.source))
      .join(' · ')
  );

  $effect(() => {
    for (const g of shown) if (!g.screen) ensureGameIcon(g.source);
  });

  // Margen mínimo contra los bordes de la ventana.
  const EDGE = 8;
  let menuPos = $state<{ x: number; y: number } | null>(null);
  let menuEl = $state<HTMLElement | null>(null);

  // Flotando se posiciona en coordenadas de viewport (position: fixed), así que no lo recorta el
  // scroller ni lo desplaza la tarjeta. Se corrige tras medirlo: el alto solo se conoce montado.
  $effect(() => {
    const el = menuEl;
    const pos = menuPos;
    if (!el || !pos) return;
    const r = el.getBoundingClientRect();
    const x = Math.max(EDGE, Math.min(pos.x, window.innerWidth - r.width - EDGE));
    // Hacia arriba si no cabe debajo, que es lo que hace el menú nativo de Windows.
    const y =
      pos.y + r.height + EDGE > window.innerHeight ? Math.max(EDGE, pos.y - r.height) : pos.y;
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
  });

  function toggleMenu(e: MouseEvent) {
    e.stopPropagation();
    menuPos = null;
    menu.openId = open ? null : playlist.id;
  }

  // El clic derecho abre el mismo menú que los tres puntos, pero en la punta del cursor.
  function onContextMenu(e: MouseEvent) {
    e.preventDefault();
    menuPos = { x: e.clientX, y: e.clientY };
    menu.openId = playlist.id;
  }

  async function onDelete(e: MouseEvent) {
    e.stopPropagation();
    menu.openId = null;
    // Shift salta la confirmación, igual que en las tarjetas de clip.
    if (!e.shiftKey && !(await confirmDeletePlaylist(playlist.name))) return;
    await deletePlaylist(playlist.id);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Enter' && e.key !== ' ') return;
    e.preventDefault();
    onopen(playlist);
  }
</script>

<svelte:window onclick={() => (menu.openId = null)} />

<div
  class="pcard"
  class:open
  role="button"
  tabindex="0"
  aria-label={playlist.name}
  onclick={(e) => {
    if ((e.target as HTMLElement).closest('.actions')) return;
    onopen(playlist);
  }}
  onkeydown={onKey}
  oncontextmenu={onContextMenu}
>
  <div class="cover">
    {#if playlist.coverSrc}
      <img src={playlist.coverSrc} alt="" draggable="false" />
    {:else}
      <Icon name="heart-fill" size={22} />
    {/if}
  </div>

  <div class="info">
    <h3 class="name">{playlist.name}</h3>
    <span class="sub mono">
      {t(clips.length === 1 ? 'pl.oneClip' : 'pl.nClips', { n: clips.length })}
      {#if total > 0}<span class="dot">•</span>{formatDuration(total)}{/if}
    </span>
  </div>

  {#if shown.length > 0}
    <div class="games" aria-label={t('pl.gamesInside')}>
      {#each shown as g, i (g.source)}
        {@const last = overflow > 0 && i === SLOTS - 1}
        <span class="slot">
          <span class="ico" class:dim={last}>
            {#if g.screen}
              <Icon name="monitor-fill" size={17} sw={1.8} />
            {:else if gameIcon(g.source) ?? groupCover(clips, g.source)}
              <img src={gameIcon(g.source) ?? groupCover(clips, g.source)} alt="" draggable="false" />
            {:else}
              <span class="ini mono">{initial(g.source)}</span>
            {/if}
            {#if last}
              <span class="more mono">+{overflow}</span>
            {/if}
          </span>
          <span class="tip" role="tooltip">{last ? hiddenNames : displaySource(g.source)}</span>
        </span>
      {/each}
    </div>
  {/if}

  <div class="actions">
    <button
      class="act"
      aria-label={t('card.more')}
      aria-haspopup="menu"
      aria-expanded={open}
      onclick={toggleMenu}
    >
      <Icon name="more" size={21} />
    </button>

    {#if open}
      <div
        class="menu"
        class:floating={!!menuPos}
        style={menuPos ? `left:${menuPos.x}px;top:${menuPos.y}px` : ''}
        bind:this={menuEl}
        role="menu"
        use:flip
      >
        <button
          role="menuitem"
          onclick={(e) => {
            e.stopPropagation();
            menu.openId = null;
            openPlaylistEdit(playlist.id);
          }}
        >
          <Icon name="rename" size={16} sw={2} /> {t('pl.edit')}
        </button>
        <div class="sep"></div>
        <button role="menuitem" class="danger" onclick={onDelete}>
          <Icon name="trash" size={16} sw={2} /> {t('pl.delete')}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .pcard {
    position: relative;
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 11px 10px 11px 12px;
    background: var(--surface);
    border-radius: 4px;
    box-shadow: 0 0 0 2px var(--ring);
    transition: box-shadow 0.15s ease;
  }
  .pcard:hover,
  .pcard.open {
    box-shadow: 0 0 0 4px var(--ring-strong);
  }
  /* Sin esto, la tarjeta siguiente (posterior en el DOM) taparía un tooltip que se salga. */
  .pcard:hover {
    z-index: 20;
  }
  .pcard.open {
    z-index: 50;
  }
  .pcard:focus-visible {
    box-shadow: 0 0 0 4px var(--accent);
    outline: none;
  }

  /* Cuadrada y del alto del bloque de texto: es la identidad de la playlist, no una portada
     de contenido, así que no compite con la rejilla de juegos de la derecha. */
  .cover {
    flex: none;
    width: 46px;
    height: 46px;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 8px;
    color: var(--text-3);
    background: var(--bg-2);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .info {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .name {
    font-size: 16px;
    font-weight: 560;
    line-height: 1.2;
    color: var(--text-0);
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

  .games {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }
  /* El tooltip vive fuera del icono: `.ico` recorta (overflow) para redondear la carátula y
     dentro quedaría cortado. */
  .slot {
    position: relative;
    display: grid;
    place-items: center;
  }
  .tip {
    position: absolute;
    bottom: calc(100% + 7px);
    left: 50%;
    transform: translateX(-50%);
    width: max-content;
    max-width: 240px;
    padding: 7px 11px;
    font-size: 12.5px;
    line-height: 1.35;
    text-align: center;
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    box-shadow: var(--shadow-float);
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    z-index: 60;
  }
  .slot:hover .tip {
    opacity: 1;
    visibility: visible;
  }
  .ico {
    position: relative;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 7px;
    color: var(--text-2);
    background: var(--bg-2);
  }
  .ico img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  /* Recorte a la altura de la mayúscula para que las iniciales queden en el centro óptico. */
  .ini {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    line-height: 1;
    text-box: trim-both cap alphabetic;
    color: var(--text-2);
  }
  /* Se atenúa la ilustración, no el hueco: el número va encima y tiene que quedar legible. */
  .ico.dim img,
  .ico.dim .ini,
  .ico.dim :global(svg) {
    opacity: 0.55;
  }
  /* El recorte de caja va aquí por lo mismo que en la duración de la tarjeta de clip: ni el
     "+" ni los dígitos tienen descendente, y el hueco que la fuente le reserva descolgaba el
     texto del centro del hueco. Donde no se soporte, queda el centrado de antes. */
  .more {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 600;
    line-height: 1;
    text-box: trim-both cap alphabetic;
    color: var(--text-0);
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.85);
  }

  .actions {
    position: relative;
    flex: none;
  }
  .act {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--r-sm);
    color: var(--text-2);
    transition: background 0.14s ease, color 0.14s ease;
  }
  .act:hover {
    background: var(--bg-hover);
    color: var(--text-0);
  }

  .menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    left: auto;
    width: 180px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    z-index: 40;
  }
  .menu.floating {
    position: fixed;
    right: auto;
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    padding: 9px 10px;
    font-size: 13px;
    color: var(--text-1);
    text-align: left;
    border-radius: 6px;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .menu button :global(svg) {
    flex-shrink: 0;
    width: 17px;
  }
  .menu button:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .menu .danger {
    color: var(--rec);
  }
  .menu .danger:hover {
    background: color-mix(in srgb, var(--rec) 12%, transparent);
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
  }
</style>
