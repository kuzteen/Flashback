<script lang="ts">
  import { untrack } from 'svelte';
  import Icon from './Icon.svelte';
  import PlaylistPicker from './PlaylistPicker.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { revealItemInDir } from '@tauri-apps/plugin-opener';
  import { menu } from '$lib/menu.svelte';
  import { formatDuration, formatRelative, displaySource, isScreenSource, type Clip } from '$lib/clips';
  import { gameIcon, ensureGameIcon } from '$lib/artwork.svelte';
  import {
    isFavorite,
    toggleFavorite,
    requestThumb,
    cachedThumb,
    refreshLibrary,
    renameFavorite,
    removeFavorite
  } from '$lib/library.svelte';
  import { openEditor } from '$lib/editor.svelte';
  import { openShare } from '$lib/share.svelte';
  import { selected, isSelected, pick } from '$lib/selection.svelte';
  import { confirmDelete } from '$lib/confirm.svelte';
  import { playlistsWith } from '$lib/playlists.svelte';
  import { t } from '$lib/i18n.svelte';

  let { clip }: { clip: Clip } = $props();

  const open = $derived(menu.openId === clip.id);
  // Margen mínimo contra los bordes de la ventana, para el menú y su submenú.
  const EDGE = 8;
  const favorite = $derived(isFavorite(clip.id));
  const sel = $derived(isSelected(clip.id));
  const picking = $derived(selected.size > 0);

  let cardEl = $state<HTMLElement | null>(null);
  // untrack: la tarjeta va keyed por clip.id, así que el valor inicial basta y leerlo aquí
  // no debe crear dependencia.
  let poster = $state<string | null>(untrack(() => (clip.path ? cachedThumb(clip.path) : null)));
  // El vídeo solo se monta en hover (sin precarga); videoReady marca cuándo ya tiene su
  // primer frame para fundirlo sobre el póster, que no se oculta y así nunca pasa por negro.
  // Al salir se desvanece y se desmonta al acabar el fundido, liberando el decodificador.
  let videoMounted = $state(false);
  let videoReady = $state(false);
  let videoEl = $state<HTMLVideoElement | null>(null);
  let unmountTimer: ReturnType<typeof setTimeout> | undefined;

  // Permanencia mínima antes de montar el <video>. Recorrer la rejilla deprisa (Tab mantenido,
  // que repite ~30 veces por segundo, o el puntero barriendo) encendía una tarjeta tras otra, y
  // como el desmontaje espera 250 ms se acumulaban varios decodificadores vivos a la vez.
  const DWELL_MS = 140;
  let activateTimer: ReturnType<typeof setTimeout> | undefined;

  function activate() {
    clearTimeout(unmountTimer);
    if (videoMounted || activateTimer) return;
    activateTimer = setTimeout(() => {
      activateTimer = undefined;
      videoMounted = true;
      if (videoEl && videoEl.readyState >= 2) videoReady = true;
    }, DWELL_MS);
  }

  function release() {
    clearTimeout(activateTimer);
    activateTimer = undefined;
    if (!videoMounted) return;
    videoReady = false;
    clearTimeout(unmountTimer);
    unmountTimer = setTimeout(() => (videoMounted = false), 250);
  }

  // El foco se comporta como el puntero, y además retiene: mientras la tarjeta lo tenga, sacar
  // el ratón no la apaga, y mientras el ratón esté encima, perder el foco tampoco. La suelta el
  // último de los dos en irse.
  let hovered = false;

  function onEnter() {
    hovered = true;
    activate();
  }

  function onLeave() {
    hovered = false;
    if (cardEl?.contains(document.activeElement)) return;
    release();
  }

  function onFocusOut(e: FocusEvent) {
    const next = e.relatedTarget as Node | null;
    if (hovered || (next && cardEl?.contains(next))) return;
    release();
  }

  function onCardKey(e: KeyboardEvent) {
    if (e.target !== cardEl || (e.key !== 'Enter' && e.key !== ' ')) return;
    e.preventDefault();
    if (picking) pick(clip.id, e.shiftKey);
    else openEditor(clip);
  }

  $effect(() => () => {
    clearTimeout(unmountTimer);
    clearTimeout(activateTimer);
  });

  // Carátula perezosa: la miniatura (un JPEG ligero cacheado por el backend) se pide solo
  // cuando la tarjeta se acerca al viewport. El <video> no se monta hasta el hover, así que
  // nunca hay decenas de decodificadores de vídeo activos a la vez.
  $effect(() => {
    const el = cardEl;
    if (!el || poster || !clip.path) return;
    // El observador solo se suelta cuando hay miniatura: si la petición falla (el backend puede
    // rechazarla bajo carga), antes se quedaba en blanco para siempre. Al no desconectar, basta
    // con que la tarjeta vuelva a entrar en el viewport para reintentarlo, y como el callback
    // solo salta en los cruces, un fallo no puede encadenar reintentos.
    let asked = false;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0].isIntersecting || asked) return;
        asked = true;
        requestThumb(clip.path).then((u) => {
          if (u) {
            poster = u;
            io.disconnect();
          } else {
            asked = false;
          }
        });
      },
      { rootMargin: '300px' }
    );
    io.observe(el);
    return () => io.disconnect();
  });

  // Submenú lateral: sale pegado al panel, alineado con su fila. Se abre al posarse encima y
  // se cierra con retardo, para que el recorrido en diagonal hasta él no lo apague a medias.
  let plOpen = $state(false);
  let plItemEl = $state<HTMLElement | null>(null);
  let plEl = $state<HTMLElement | null>(null);
  let plHoverTimer: ReturnType<typeof setTimeout> | undefined;
  let plCloseTimer: ReturnType<typeof setTimeout> | undefined;
  const inPlaylists = $derived(playlistsWith(clip.path).length > 0);

  const PL_OPEN_MS = 110;
  const PL_CLOSE_MS = 220;

  function plEnter() {
    clearTimeout(plCloseTimer);
    if (plOpen || plHoverTimer) return;
    plHoverTimer = setTimeout(() => {
      plHoverTimer = undefined;
      plOpen = true;
    }, PL_OPEN_MS);
  }

  function plLeave() {
    clearTimeout(plHoverTimer);
    plHoverTimer = undefined;
    clearTimeout(plCloseTimer);
    plCloseTimer = setTimeout(() => (plOpen = false), PL_CLOSE_MS);
  }

  $effect(() => {
    if (!open) {
      plOpen = false;
      clearTimeout(plHoverTimer);
      clearTimeout(plCloseTimer);
    }
  });

  $effect(() => () => {
    clearTimeout(plHoverTimer);
    clearTimeout(plCloseTimer);
  });

  // Se coloca en coordenadas de viewport y se mide ya montado: el alto depende de cuántas
  // playlists haya, y el lado de cuánto sitio quede. Primero intenta la derecha (que es donde
  // se espera); si no cabe, se vuelca a la izquierda del panel, y solo si tampoco cabe ahí se
  // pega al borde. En vertical se empuja hacia arriba lo justo para entrar en pantalla.
  $effect(() => {
    const el = plEl;
    const item = plItemEl;
    const panel = menuEl;
    if (!el || !item || !panel) return;
    const p = panel.getBoundingClientRect();
    const i = item.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    const GAP = 6;
    let x = p.right + GAP;
    if (x + r.width + EDGE > window.innerWidth) {
      const flipped = p.left - GAP - r.width;
      x = flipped >= EDGE ? flipped : Math.max(EDGE, window.innerWidth - r.width - EDGE);
    }
    // -5 px: el relleno del panel, para que la primera fila del submenú quede a la altura de
    // la fila que lo abre y no un pelo por debajo.
    let y = Math.min(i.top - 5, window.innerHeight - r.height - EDGE);
    y = Math.max(EDGE, y);
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
    el.style.visibility = 'visible';
  });

  // La caché de iconos es la misma que usan las playlists: si ya se pidió, no hay IPC nuevo.
  $effect(() => {
    if (clip.source && !isScreenSource(clip.source)) ensureGameIcon(clip.source);
  });

  function toggleMenu(e: MouseEvent) {
    e.stopPropagation();
    menuPos = null;
    menu.openId = open ? null : clip.id;
  }

  // El clic derecho abre el mismo menú que los tres puntos, pero en la punta del cursor. Sobre el
  // campo de renombrar se deja pasar para no perder el copiar/pegar nativo, igual que hace el
  // guard global del layout.
  function onContextMenu(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('input, textarea, [contenteditable="true"]')) return;
    e.preventDefault();
    menuPos = { x: e.clientX, y: e.clientY };
    menu.openId = clip.id;
  }

  // Flotando se posiciona en coordenadas de viewport (position: fixed), así que no lo recorta el
  // scroller ni lo desplaza la tarjeta. Se corrige tras medirlo: el alto depende del menú y solo
  // se conoce una vez montado.
  let menuPos = $state<{ x: number; y: number } | null>(null);
  let menuEl = $state<HTMLElement | null>(null);

  $effect(() => {
    const el = menuEl;
    const pos = menuPos;
    if (!el || !pos) return;
    const r = el.getBoundingClientRect();
    const x = Math.max(EDGE, Math.min(pos.x, window.innerWidth - r.width - EDGE));
    // Hacia arriba si no cabe debajo, que es lo que hace el menú nativo de Windows.
    const y =
      pos.y + r.height + EDGE > window.innerHeight
        ? Math.max(EDGE, pos.y - r.height)
        : pos.y;
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
  });

  // Abrir el editor al pulsar la tarjeta, salvo cuando el click nace dentro del menú de
  // acciones: soltar el botón en una opción distinta de donde se pulsó sintetiza un click
  // sobre el contenedor del menú, y sin esta guarda subiría hasta aquí y abriría el editor.
  function openFromCard(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('.actions')) return;
    // Con una selección en curso, pulsar la tarjeta marca en vez de abrir: si no, es
    // facilísimo salirse al editor a mitad de seleccionar.
    if (picking) {
      pick(clip.id, e.shiftKey);
      return;
    }
    openEditor(clip);
  }

  function pickClick(e: MouseEvent) {
    e.stopPropagation();
    pick(clip.id, e.shiftKey);
  }

  function favClick(e: MouseEvent) {
    e.stopPropagation();
    menu.openId = null;
    toggleFavorite(clip.id);
  }

  let renaming = $state(false);
  let renameValue = $state('');

  function startRename(e: MouseEvent) {
    e.stopPropagation();
    menu.openId = null;
    renameValue = clip.title;
    renaming = true;
  }

  async function commitRename() {
    if (!renaming) return;
    renaming = false;
    const name = renameValue.trim();
    if (!name || name === clip.title) return;
    try {
      const newPath = await invoke<string>('rename_clip', { path: clip.path, newName: name });
      const newId = newPath.split(/[\\/]/).pop() ?? clip.id;
      renameFavorite(clip.id, newId);
      await refreshLibrary();
    } catch (err) {
      console.error('rename_clip', err);
    }
  }

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function openLocation(e: MouseEvent) {
    e.stopPropagation();
    menu.openId = null;
    try {
      await revealItemInDir(clip.path);
    } catch (err) {
      console.error('revealItemInDir', err);
    }
  }

  async function deleteClip(e: MouseEvent) {
    e.stopPropagation();
    menu.openId = null;
    // Shift salta la confirmación: el borrado va a la papelera, así que el atajo no es
    // irreversible para quien ya sabe lo que hace.
    if (!e.shiftKey && !(await confirmDelete(1, clip.title))) return;
    try {
      await invoke('delete_clip', { path: clip.path });
      selected.delete(clip.id);
      removeFavorite(clip.id);
      await refreshLibrary();
    } catch (err) {
      console.error('delete_clip', err);
    }
  }
</script>

<svelte:window onclick={() => (menu.openId = null)} />

<div
  class="card"
  class:open
  class:sel
  bind:this={cardEl}
  role="button"
  tabindex="0"
  aria-label={clip.title}
  onmouseenter={onEnter}
  onmouseleave={onLeave}
  onfocusin={activate}
  onfocusout={onFocusOut}
  onclick={openFromCard}
  oncontextmenu={onContextMenu}
  onkeydown={onCardKey}
>
  <div class="thumb">
    {#if poster}
      <img class="preview" src={poster} alt="" draggable="false" />
    {:else}
      <div class="watermark"><Icon name="chevrons" size={150} sw={1.1} /></div>
    {/if}
    {#if videoMounted && clip.previewSrc}
      <video
        bind:this={videoEl}
        class="preview vid"
        class:show={videoReady}
        src={clip.previewSrc}
        muted
        loop
        playsinline
        autoplay
        preload="none"
        onloadeddata={() => (videoReady = true)}
      ></video>
    {/if}
    <div class="scrim"></div>

    <div class="badge mono">
      <!-- El recorte es lineal y pesa menos que la estrella y la goma, macizas: va algo mayor. -->
      {#if favorite}<span class="fav"><Icon name="star-fill" size={12} /></span>{/if}
      {#if clip.exported}<Icon name="crop" size={13} />{/if}
      {#if clip.edited}<Icon name="eraser" size={12} />{/if}
      <span class="dur">{formatDuration(clip.durationSec)}</span>
    </div>

  </div>

  <button
    class="pick"
    class:armed={picking}
    role="checkbox"
    aria-checked={sel}
    aria-label={t('card.select')}
    onclick={pickClick}
  >
    <Icon name="check" size={14} sw={2.8} />
  </button>

  <div class="meta">
    <div class="info">
      {#if clip.source}
        <span class="src label">
          {#if isScreenSource(clip.source)}
            <Icon name="monitor-fill" size={15} sw={1.8} />
          {:else if gameIcon(clip.source)}
            <img class="src-ico" src={gameIcon(clip.source)} alt="" draggable="false" />
          {/if}
          {displaySource(clip.source)}
        </span>
      {/if}

      <h3 class="title">{clip.title}</h3>

      {#if renaming}
        <input
          class="title-edit"
          bind:value={renameValue}
          use:focusSelect
          onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => {
            e.stopPropagation();
            if (e.key === 'Enter') commitRename();
            else if (e.key === 'Escape') renaming = false;
          }}
          onblur={commitRename}
        />
      {/if}

      <span class="when mono"><Icon name="clock" size={13} sw={2} />{formatRelative(clip.createdAt)}</span>
    </div>

    <div class="actions">
      <button
        class="act"
        aria-label={t('card.share')}
        onclick={(e) => {
          e.stopPropagation();
          openShare(clip);
        }}
      >
        <Icon name="share" size={21} />
      </button>
      <button
        class="act"
        aria-label={t('card.more')}
        aria-haspopup="menu"
        aria-expanded={open}
        onclick={toggleMenu}
      >
        <Icon name="more" size={23} />
      </button>

      {#if open}
        <div
          class="menu"
          class:floating={!!menuPos}
          style={menuPos ? `left:${menuPos.x}px;top:${menuPos.y}px` : ''}
          bind:this={menuEl}
          role="menu"
        >
          <button role="menuitem" onclick={(e) => { e.stopPropagation(); openEditor(clip); }}><Icon name="editor" size={16} /> {t('card.openEditor')}</button>
          <button role="menuitem" class:on={favorite} onclick={favClick}><Icon name="star-fill" size={16} /> {favorite ? t('card.favRemove') : t('card.favAdd')}</button>
          <button
            class="sub-item"
            class:on-pl={inPlaylists}
            class:open={plOpen}
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={plOpen}
            bind:this={plItemEl}
            onmouseenter={plEnter}
            onmouseleave={plLeave}
            onfocus={plEnter}
            onclick={(e) => {
              e.stopPropagation();
              clearTimeout(plHoverTimer);
              plOpen = !plOpen;
            }}
          >
            <Icon name="folder-plus" size={16} />
            {t('pl.addTo')}
            <Icon name="chevron-down" size={13} sw={2.2} />
          </button>
          <button role="menuitem" onclick={startRename}><Icon name="rename" size={16} sw={2} /> {t('card.rename')}</button>
          <button role="menuitem" onclick={openLocation}><Icon name="folder-open" size={16} sw={2} /> {t('card.openLocation')}</button>
          <div class="sep"></div>
          <button role="menuitem" class="danger" onclick={deleteClip}><Icon name="trash" size={16} sw={2} /> {t('card.delete')}</button>

          {#if plOpen}
            <div
              class="submenu"
              role="menu"
              tabindex="-1"
              bind:this={plEl}
              onmouseenter={() => clearTimeout(plCloseTimer)}
              onmouseleave={plLeave}
            >
              <PlaylistPicker paths={[clip.path]} onclose={() => (plOpen = false)} />
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .card {
    position: relative;
    background: var(--surface);
    border-radius: 4px;
    box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.08);
    transition: box-shadow 0.15s ease;
  }
  /* El borde es una sola sombra sólida por fuera de la card: en hover solo crece su spread, que
     es pintado puro (sin layout), así nada se mueve y nunca hay dos piezas con tonos distintos. */
  .card:hover,
  .card.open {
    box-shadow: 0 0 0 4px rgba(255, 255, 255, 0.16);
  }
  /* Por encima de la barra de selección (z-index 40): con el clic derecho el menú puede caer
     justo donde está, y quedar por debajo lo dejaría a medias. */
  .card.open {
    z-index: 50;
  }

  .thumb {
    position: relative;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border-radius: 4px 4px 0 0;
    background: #000;
  }
  .watermark {
    position: absolute;
    right: -26px;
    bottom: -34px;
    color: #ffffff;
    opacity: 0.07;
    transform: rotate(-8deg);
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, rgba(0, 0, 0, 0.28), transparent 30%, transparent 62%, rgba(0, 0, 0, 0.34));
  }

  .preview {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .vid {
    opacity: 0;
    transition: opacity 0.25s ease;
  }
  .vid.show {
    opacity: 1;
  }

  .badge {
    position: absolute;
    top: 10px;
    right: 10px;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 9px;
    font-size: 12px;
    color: var(--text-0);
    background: rgba(27, 30, 38, 0.6);
    backdrop-filter: blur(6px);
    border: 1px solid var(--line);
    border-radius: 999px;
  }
  /* La altura la fija la píldora, no el interlineado. text-box recorta la caja del texto a la
     altura de mayúsculas: los dígitos no tienen descendente, así que sin recortar el hueco que
     este reserva los descolgaba del centro. Donde no se soporte, el centrado es el de antes. */
  .badge .dur {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .badge :global(svg) {
    color: var(--bright);
    flex: none;
  }
  .fav {
    display: flex;
  }
  .fav :global(svg) {
    color: var(--gold);
  }

  .pick {
    position: absolute;
    top: 10px;
    left: 10px;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-radius: 7px;
    color: transparent;
    background: rgba(4, 8, 14, 0.55);
    backdrop-filter: blur(6px);
    border: 1px solid var(--line-strong);
    opacity: 0;
    transition: opacity 0.16s ease, background 0.14s ease, border-color 0.14s ease;
  }
  /* Al llegar con Tab no hay puntero que descubra el check, y sin él la tarjeta parece no tener
     forma de seleccionarse. :focus-visible y no :focus-within: pulsar un botón con el ratón
     también deja el foco dentro, y entonces el check se quedaba clavado al apartar el ratón.
     El :has() va en su propia regla porque un navegador que no lo entienda tiraría el bloque
     entero, y con él el hover. */
  .card:hover .pick,
  .card:focus-visible .pick,
  .pick.armed {
    opacity: 1;
  }
  .card:has(:focus-visible) .pick {
    opacity: 1;
  }
  .card.sel .pick {
    opacity: 1;
    color: var(--base);
    background: var(--bright);
    border-color: var(--bright);
  }
  .card.sel {
    box-shadow: 0 0 0 2px var(--bright);
  }
  /* El foco reutiliza el anillo del hover subido al tono del outline, en vez de dibujar un
     segundo borde por fuera. Va después de .sel para ganarle: el check relleno ya dice que el
     clip está marcado. Solo :focus-visible: el anillo es la guía del teclado, y con el ratón
     (pulsar un botón, abrir el menú con el derecho) no debe aparecer. */
  .card:focus-visible {
    box-shadow: 0 0 0 4px var(--accent);
    outline: none;
  }
  .card:has(:focus-visible) {
    box-shadow: 0 0 0 4px var(--accent);
    outline: none;
  }

  .meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 11px 14px;
  }
  /* Rejilla de 3 filas con la del título fijada al centro: el origen empuja hacia arriba y la
     fecha hacia abajo, así el título queda a la misma altura haya o no origen. min-height fija la
     altura del pie para que los botones de la derecha no se muevan entre tarjetas. */
  .info {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 52px;
    display: grid;
    grid-template-rows: 1fr auto 1fr;
  }
  /* El icono va un poco por encima del texto (16 contra 12): a tamaño de texto una carátula no
     se distingue, y cuatro píxeles bastan para reconocerla sin desequilibrar la fila. */
  .src {
    display: flex;
    align-items: center;
    gap: 6px;
    align-self: end;
    padding-bottom: 6px;
    line-height: 1;
    font-size: 12px;
    color: var(--text-2);
    min-width: 0;
  }
  .src-ico {
    flex: none;
    width: 16px;
    height: 16px;
    object-fit: cover;
    border-radius: 4px;
    display: block;
  }
  .title {
    font-size: 16px;
    font-weight: 560;
    line-height: 1.2;
    color: var(--text-0);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* El campo flota sobre el título en vez de sustituirlo: ocupando sitio en la rejilla, su
     borde y su relleno estiraban la fila y la tarjeta crecía 8 px al renombrar. Las filas 1fr de
     arriba y abajo son iguales, así que el centro de .info es el centro de la fila del título.
     Los -6 px laterales compensan el relleno para que el texto no se desplace al aparecer. */
  .title-edit {
    position: absolute;
    left: -6px;
    right: -6px;
    top: 50%;
    transform: translateY(-50%);
    min-width: 0;
    font-size: 16px;
    font-weight: 560;
    line-height: 1.2;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--accent);
    border-radius: 5px;
    padding: 3px 6px;
    outline: none;
  }

  .actions {
    position: relative;
    display: flex;
    align-self: center;
    gap: 2px;
    flex-shrink: 0;
  }
  .act {
    width: 38px;
    height: 38px;
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

    font-family: var(--font-display);
  .menu {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    left: auto;
    width: 196px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: 0 18px 42px -14px rgba(0, 0, 0, 0.7);
    z-index: 40;
  }
  .menu.floating {
    position: fixed;
    right: auto;
    font-family: var(--font-display);
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
  /* Hueco fijo para el icono: los glifos no miden lo mismo y sin esto los textos del menú no
     quedarían alineados entre sí. */
  .menu button :global(svg) {
    flex-shrink: 0;
    width: 17px;
  }
  .menu button:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .menu button.on :global(svg) {
    color: var(--gold);
  }
  .menu button.on-pl > :global(svg:first-child) {
    color: var(--bright);
  }
  /* La flecha del submenú empuja a la derecha con margin-left:auto; el chevron del set apunta
     hacia abajo, así que se gira. Mantiene el color del texto, no el del icono de la izquierda. */
  .sub-item > :global(svg:last-child) {
    margin-left: auto;
    width: auto;
    transform: rotate(-90deg);
    color: var(--text-3);
  }
  .menu .sub-item.open {
    background: var(--bg-3);
    color: var(--text-0);
  }

  /* Fijo respecto al viewport: el panel que lo abre puede estar anclado a la tarjeta o flotando
     en el cursor, y así el mismo cálculo vale para los dos. Arranca oculto porque hay que
     medirlo antes de saber de qué lado cae. */
  .submenu {
    position: fixed;
    left: 0;
    top: 0;
    visibility: hidden;
    width: 216px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: 0 18px 42px -14px rgba(0, 0, 0, 0.7);
    z-index: 41;
  }
  .menu .danger {
    color: var(--rec);
  }
  .menu .danger:hover {
    background: rgba(255, 91, 91, 0.12);
    color: var(--rec);
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
  }

  /* height fija a la altura del texto (11px): el reloj es un poco más alto y, sin esto, empujaría
     la línea hacia abajo y la fecha quedaría más lejos del título que el origen. */
  .when {
    display: flex;
    align-self: end;
    align-items: center;
    height: 11px;
    gap: 6px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    line-height: 1;
    color: var(--text-2);
  }
  .when :global(svg) {
    flex-shrink: 0;
  }
</style>
