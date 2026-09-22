<script lang="ts">
  import Icon from '../Icon.svelte';
  import { closeEditor, editorState, persistEdit } from '$lib/editor-state.svelte';
  import { matchShortcut } from '$lib/shortcuts';
  import { shareState } from '$lib/share.svelte';
  import { t } from '$lib/i18n.svelte';
  import { runAction } from './actions';
  import { playback } from './playback.svelte';
  import { ui } from './ui.svelte';
  import EditorHeader from './EditorHeader.svelte';
  import Viewer from './Viewer.svelte';
  import Transport from './Transport.svelte';
  import Timeline from './Timeline.svelte';
  import OutputBar from './OutputBar.svelte';
  import FormatPanel from './FormatPanel.svelte';

  let root = $state<HTMLDivElement | null>(null);
  let dock = $state<HTMLDivElement | null>(null);
  let dockDrag: { startY: number; startH: number } | null = null;

  // El layout monta un editor por clip (keyed por id): se arranca limpio y se deja limpio. El
  // reinicio va en el cuerpo del script y no en un efecto porque este corre antes de montar los
  // hijos; en un efecto podría pisar lo que el visor acaba de configurar.
  ui.reset();
  playback.reset();
  $effect(() => () => {
    playback.reset();
    ui.reset();
  });

  // El editor se abre con un clic en una tarjeta y el foco se quedaba en ella, detrás: Espacio le
  // llegaba primero y la tarjeta lo tomaba como "abrir", reiniciando el editor con el mismo clip
  // (sin volver a cargar el vídeo, así que el montaje quedaba vacío). Con el foco dentro del
  // editor, ninguna tecla llega a lo que hay detrás.
  $effect(() => {
    root?.focus({ preventScroll: true });
  });

  // En pantalla completa, reproducir o pausar vuelve a enseñar los controles (y reprograma su
  // ocultación si está sonando).
  $effect(() => {
    void playback.playing;
    if (ui.fs) ui.showFsCtrl();
  });

  async function close() {
    playback.pause();
    if (ui.fs) await ui.setFs(false);
    await persistEdit();
    closeEditor();
  }

  function onKey(e: KeyboardEvent) {
    // Con el diálogo de compartir delante el editor no escucha; defaultPrevented cubre el Escape
    // que el diálogo ya consumió.
    if (shareState.clip || e.defaultPrevented) return;
    const el = e.target as HTMLElement | null;
    if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      if (ui.toolsOpen) ui.toolsOpen = false;
      else if (ui.blockMenu) ui.blockMenu = null;
      else if (ui.range) ui.range = null;
      else if (ui.fs) void ui.setFs(false);
      else void close();
      return;
    }
    const id = matchShortcut(e);
    if (!id) return;
    // Con un fader enfocado, las flechas mueven el volumen y no el fotograma.
    if ((id === 'prevFrame' || id === 'nextFrame') && el?.getAttribute('role') === 'slider') return;
    e.preventDefault();
    runAction(id);
  }

  function onGripDown(e: PointerEvent) {
    if (e.button !== 0 || !dock) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dockDrag = { startY: e.clientY, startH: dock.getBoundingClientRect().height };
  }

  // Subir el tirador agranda el panel y encoge el vídeo; se dejan mínimos para los dos.
  function onGripMove(e: PointerEvent) {
    if (!dockDrag) return;
    const avail = root?.clientHeight ?? window.innerHeight;
    const max = Math.max(220, avail - 56 - 200);
    const next = dockDrag.startH + (dockDrag.startY - e.clientY);
    ui.dockH = Math.round(Math.max(220, Math.min(next, max)));
  }

  function onGripUp() {
    dockDrag = null;
  }
</script>

<svelte:window onkeydown={onKey} onmousemove={() => ui.fs && ui.showFsCtrl()} />
<svelte:document onmouseleave={() => ui.hideFsCtrl()} />

<div class="ed" bind:this={root} tabindex="-1">
  <EditorHeader onclose={close} />
  <div class="middle">
    <Viewer />
    <FormatPanel />
  </div>
  <div class="dock" bind:this={dock} style:height={ui.dockH !== null ? `${ui.dockH}px` : null}>
    <div
      class="grip"
      role="presentation"
      onpointerdown={onGripDown}
      onpointermove={onGripMove}
      onpointerup={onGripUp}
      onpointercancel={onGripUp}
    ></div>
    <Transport />
    <Timeline />
    <OutputBar />
  </div>

  {#if ui.notice}
    <div class="notice mono">{ui.notice}</div>
  {/if}

  {#if ui.shot}
    <div class="shot" class:fs={ui.fs}>
      <span class="check"><Icon name="camera" size={15} /></span>
      <span>{t('ed.shotSaved')}</span>
      <button onclick={() => ui.openShot()}>{t('ed.shotOpen')}</button>
    </div>
  {/if}

  {#if editorState.exporting}
    <div class="export-backdrop">
      <div class="export-card">
        <div class="export-title">{t('ed.exportingClip')}</div>
        <div class="export-bar">
          <div class="export-fill" style:width="{Math.max(2, Math.round(editorState.exportProgress * 100))}%"></div>
        </div>
        <div class="export-pct mono">{Math.round(editorState.exportProgress * 100)}%</div>
      </div>
    </div>
  {/if}
</div>

<style>
  .ed {
    position: fixed;
    left: 0;
    right: 0;
    top: calc(var(--topbar-h, 60px) - 1px);
    bottom: 0;
    z-index: 100;
    display: flex;
    flex-direction: column;
    background: var(--bg-0);
    outline: none;
  }
  .middle {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .dock {
    position: relative;
    flex: none;
    height: 340px;
    display: flex;
    flex-direction: column;
    background: var(--bg-0);
  }
  .grip {
    position: absolute;
    top: -5px;
    left: 0;
    right: 0;
    height: 10px;
    z-index: 5;
    cursor: ns-resize;
    touch-action: none;
  }
  /* Asa visible centrada, encima del play: sin ella nada decía que el borde se puede arrastrar.
     La franja de agarre sigue siendo todo el ancho. */
  .grip::after {
    content: '';
    position: absolute;
    top: 3px;
    left: 50%;
    width: 36px;
    height: 4px;
    margin-left: -18px;
    border-radius: 999px;
    background: var(--line-strong);
    transition: background 0.14s ease, width 0.14s ease, margin 0.14s ease;
  }
  .grip:hover::after,
  .grip:active::after {
    width: 48px;
    margin-left: -24px;
    background: var(--text-3);
  }

  /* Tooltips propios: el title nativo es el de Chromium y desentona con la app. Cualquier
     control del editor los pide con data-tip; aparecen con un retraso para no encenderse al
     cruzar la barra. */
  .ed :global([data-tip]) {
    position: relative;
  }
  .ed :global([data-tip]::after) {
    content: attr(data-tip);
    position: absolute;
    bottom: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%);
    width: max-content;
    max-width: 260px;
    padding: 6px 10px;
    font-family: var(--font-ui);
    font-size: 12px;
    font-weight: 400;
    line-height: 1.3;
    letter-spacing: 0;
    text-transform: none;
    white-space: nowrap;
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--line-strong);
    border-radius: 7px;
    box-shadow: 0 12px 30px -10px rgba(0, 0, 0, 0.7);
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
    transition: opacity 0.12s ease, visibility 0.12s;
    z-index: 300;
  }
  .ed :global([data-tip]:hover::after),
  .ed :global([data-tip]:focus-visible::after) {
    opacity: 1;
    visibility: visible;
    transition-delay: 0.35s;
  }
  .ed :global([data-tip-pos='below']::after) {
    top: calc(100% + 8px);
    bottom: auto;
  }
  .ed :global([data-tip-align='start']::after) {
    left: 0;
    transform: none;
  }
  .ed :global([data-tip-align='end']::after) {
    left: auto;
    right: 0;
    transform: none;
  }

  .notice {
    position: absolute;
    left: 50%;
    bottom: 76px;
    transform: translateX(-50%);
    z-index: 120;
    padding: 8px 14px;
    font-size: 12px;
    color: var(--text-0);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    box-shadow: 0 12px 30px -10px rgba(0, 0, 0, 0.7);
  }
  .shot {
    position: absolute;
    top: 70px;
    right: 18px;
    z-index: 120;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    font-size: 13px;
    color: var(--text-0);
    background: rgba(18, 18, 20, 0.72);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
  }
  .shot.fs {
    position: fixed;
    z-index: 10001;
  }
  .check {
    display: grid;
    color: var(--accent);
  }
  .shot button {
    font-size: 12.5px;
    color: var(--text-2);
    text-decoration: underline;
  }
  .shot button:hover {
    color: var(--text-0);
  }
  .export-backdrop {
    position: absolute;
    inset: 0;
    z-index: 150;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.55);
  }
  .export-card {
    width: 320px;
    padding: 20px 22px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.8);
  }
  .export-title {
    font-size: 13.5px;
    color: var(--text-0);
  }
  .export-bar {
    height: 6px;
    border-radius: 999px;
    background: var(--bg-3);
    overflow: hidden;
  }
  .export-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s ease;
  }
  .export-pct {
    font-size: 12px;
    color: var(--text-2);
    text-align: right;
  }
</style>
