<script lang="ts">
  import { flip } from '$lib/flip';
  import '@fontsource-variable/geist';
  import '@fontsource-variable/outfit';
  import '../app.css';
  import { page } from '$app/state';
  import { afterNavigate, beforeNavigate, goto } from '$app/navigation';
  import Icon from '$lib/components/Icon.svelte';
  import WindowControls from '$lib/components/WindowControls.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { register, unregisterAll } from '@tauri-apps/plugin-global-shortcut';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { hotkeys, capture, labelFor, labelTokens } from '$lib/hotkeys.svelte';
  import { refreshLibrary } from '$lib/library.svelte';
  import { replay, setReplaySeconds, BUFFER_OPTIONS } from '$lib/replay.svelte';
  import { gainFor, replaySound } from '$lib/replay-sound.svelte';
  import Editor from '$lib/components/editor/Editor.svelte';
  import ShareDialog from '$lib/components/ShareDialog.svelte';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import PlaylistDialog from '$lib/components/PlaylistDialog.svelte';
  import ClipEditDialog from '$lib/components/ClipEditDialog.svelte';
  import { editorState, closeEditor } from '$lib/editor-state.svelte';
  import {
    captureConfig,
    setFps,
    setQuality,
    setResolution,
    setMic,
    setMicDevice,
    qualityLabel,
    resolutionLabel,
    estimatedClipSize,
    FPS_OPTIONS,
    QUALITY_OPTIONS,
    RES_OPTIONS,
    type QualityKey
  } from '$lib/capture-config.svelte';
  import { gameSettings, loadDisabledGames } from '$lib/games.svelte';
  import { displaySource, isScreenSource } from '$lib/clips';
  import { artSrc, ensureGameHero, gameHero } from '$lib/artwork.svelte';
  import { t, initLocale } from '$lib/i18n.svelte';
  import {
    updater,
    checkForUpdate,
    maybeAutoShow,
    openUpdatePopup,
    closeUpdatePopup,
    installUpdate
  } from '$lib/updater.svelte';

  let { children } = $props();

  initLocale();

  const nav = [
    { href: '/', icon: 'clips-fill', labelKey: 'nav.clips' },
    { href: '/playlists', icon: 'folder-fill', labelKey: 'nav.playlists' }
  ];

  const isActive = (href: string) =>
    href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);

  type Monitor = {
    id: string;
    label: string;
    width: number;
    height: number;
    primary: boolean;
    thumb: string | null;
  };
  type AudioInput = { id: string; name: string };

  let monitors = $state<Monitor[]>([]);
  let selectedMonitor = $state<string | null>(null);
  let pickerOpen = $state(false);
  let recording = $state(false);
  // La grabación va colgada del replay (mismo encoder): si el replay se reinicia, se cierra antes.
  let recordingTapped = false;
  let micOn = $state(captureConfig.mic);
  let audioInputs = $state<AudioInput[]>([]);
  let micInput = $state(captureConfig.micDevice);
  let micDDOpen = $state(false);
  let settingsOpen = $state(false);
  let openRow = $state<string | null>(null);
  let gearSpin = $state(false);

  const secondsLabel = (s: number) => BUFFER_OPTIONS.find((o) => o.seconds === s)?.label ?? `${s}s`;

  // Resumen de los ajustes para el botón único de la barra: duración · calidad · resolución · fps.
  // Cada campo tiene un ancho fijo (clase qpart-*) para que nada se desplace al cambiar un valor.
  const summaryParts = $derived([
    { key: 'dur', text: secondsLabel(replay.seconds) },
    { key: 'qual', text: qualityLabel(captureConfig.quality) },
    { key: 'res', text: resolutionLabel(captureConfig.resolution) },
    { key: 'fps', text: `${captureConfig.fps} FPS` }
  ]);

  type QRow = { key: string; title: string; value: string; options: { label: string; raw: number | string }[] };

  const settingRows = $derived<QRow[]>([
    {
      key: 'tiempo',
      title: t('cap.duration'),
      value: secondsLabel(replay.seconds),
      options: BUFFER_OPTIONS.map((o) => ({ label: o.label, raw: o.seconds }))
    },
    {
      key: 'calidad',
      title: t('cap.quality'),
      value: qualityLabel(captureConfig.quality),
      options: QUALITY_OPTIONS.map((q) => ({ label: qualityLabel(q.key), raw: q.key }))
    },
    {
      key: 'resolucion',
      title: t('cap.resolution'),
      value: resolutionLabel(captureConfig.resolution),
      options: RES_OPTIONS.map((r) => ({ label: r.label, raw: r.height }))
    },
    {
      key: 'fps',
      title: t('cap.fps'),
      value: `${captureConfig.fps} FPS`,
      options: FPS_OPTIONS.map((f) => ({ label: `${f} FPS`, raw: f }))
    }
  ]);

  function toggleRow(e: MouseEvent, key: string) {
    e.stopPropagation();
    openRow = openRow === key ? null : key;
  }

  function pickRow(e: MouseEvent, key: string, raw: number | string) {
    e.stopPropagation();
    if (key === 'tiempo') setReplaySeconds(raw as number);
    else if (key === 'calidad') setQuality(raw as QualityKey);
    else if (key === 'resolucion') setResolution(raw as number);
    else if (key === 'fps') setFps(raw as number);
    openRow = null;
  }

  // Tamaño estimado de un replay de la duración seleccionada con los ajustes actuales.
  const estSize = $derived(
    estimatedClipSize(
      replay.seconds,
      captureConfig.quality,
      captureConfig.resolution,
      captureConfig.fps
    )
  );

  // El feedback se muestra como toast en una ventana overlay (transparente, siempre
  // encima, click-through) para que sea visible también sobre el juego en modo Aplicación.
  type ToastKind = 'info' | 'ready' | 'saved' | 'error';
  type ToastTopic = 'saved' | 'ready' | 'recording' | 'problems';
  function toast(text: string, topic: ToastTopic, kind: ToastKind = 'info', keys: string[] = []) {
    invoke('toast', { payload: { title: 'Flashback', body: text, keys, kind, topic } }).catch(() => {});
  }

  const activeMonitor = $derived(monitors.find((m) => m.id === selectedMonitor) ?? null);
  const micName = $derived(audioInputs.find((d) => d.id === micInput)?.name ?? t('cap.noMicsShort'));
  // El recorrido es scrollWidth - clientWidth para que el final quede al ras del borde
  // y nunca se salga de vista; la duración escala con la distancia para velocidad constante.
  function marquee(node: HTMLElement, _text: string) {
    const measure = () => {
      const over = node.scrollWidth - node.clientWidth;
      node.classList.toggle('scroll', over > 1);
      node.style.setProperty('--marq', `${-over}px`);
      node.style.setProperty('--marq-dur', `${2.5 + over / 24}s`);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(node);
    return { update: measure, destroy: () => ro.disconnect() };
  }

  async function loadMonitors() {
    try {
      monitors = await invoke<Monitor[]>('list_monitors');
    } catch {
      // fuera de Tauri (preview en navegador)
    }
  }

  async function loadAudioInputs() {
    try {
      audioInputs = await invoke<AudioInput[]>('list_audio_inputs');
      if (!audioInputs.some((d) => d.id === micInput)) {
        micInput = audioInputs[0]?.id ?? '';
        setMicDevice(micInput);
      }
    } catch {
      // fuera de Tauri (preview en navegador)
    }
  }

  function togglePicker(e: MouseEvent) {
    e.stopPropagation();
    pickerOpen = !pickerOpen;
    if (pickerOpen) {
      loadMonitors();
      loadAudioInputs();
    }
  }

  function closeAll() {
    pickerOpen = false;
    micDDOpen = false;
    settingsOpen = false;
    openRow = null;
  }

  // Desactiva el menú contextual nativo de WebView2 (atrás, recargar, guardar como, imprimir…) en
  // toda la app. Se respeta en campos de texto para no perder copiar/pegar; el editor monta su
  // propio menú en los bloques (que llama a preventDefault antes de llegar aquí).
  function onContextMenu(e: MouseEvent) {
    const t = e.target as HTMLElement | null;
    if (t?.closest('input, textarea, [contenteditable="true"]')) return;
    e.preventDefault();
  }

  function focusOnMount(node: HTMLElement) {
    node.focus({ preventScroll: true });
  }

  function onUpdateKey(e: KeyboardEvent) {
    if (!updater.popupOpen || e.key !== 'Escape') return;
    e.preventDefault();
    e.stopImmediatePropagation();
    closeUpdatePopup();
  }

  // .content es un único contenedor de scroll para todas las rutas y SvelteKit solo restaura el
  // de la ventana, así que cada ruta guarda aquí su posición. Se reintenta unos fotogramas porque
  // la rejilla virtual fija su alto después de medir la primera fila, y hasta entonces el
  // navegador recorta el scrollTop.
  let contentEl = $state<HTMLDivElement | null>(null);
  const scrollByPath = new Map<string, number>();
  let restoreRaf = 0;
  beforeNavigate(({ from }) => {
    if (contentEl && from?.url) scrollByPath.set(from.url.pathname, contentEl.scrollTop);
  });
  afterNavigate(({ from, to }) => {
    const el = contentEl;
    // En la carga inicial `from` existe pero con url null; un error aquí rompe el arranque del
    // router y cada clic en un enlace pasa a ser una recarga completa.
    if (!el || !to?.url || !from?.url || from.url.pathname === to.url.pathname) return;
    const target = scrollByPath.get(to.url.pathname) ?? 0;
    cancelAnimationFrame(restoreRaf);
    let frames = 0;
    const apply = () => {
      el.scrollTop = target;
      if (Math.abs(el.scrollTop - target) > 1 && ++frames < 30) restoreRaf = requestAnimationFrame(apply);
    };
    apply();
  });

  function toggleMicDD(e: MouseEvent) {
    e.stopPropagation();
    micDDOpen = !micDDOpen;
  }
  function pickMic(e: MouseEvent, id: string) {
    e.stopPropagation();
    micInput = id;
    setMicDevice(id);
    micDDOpen = false;
  }

  function toggleSettings(e: MouseEvent) {
    e.stopPropagation();
    settingsOpen = !settingsOpen;
  }

  // Elegir pantalla solo fija el objetivo; grabar es aparte (atajo / botón). No cierra el
  // menú (stopPropagation evita que el click del documento lo cierre).
  async function selectMonitor(e: MouseEvent, id: string) {
    e.stopPropagation();
    if (id === selectedMonitor) return;
    if (recording) await stopRecording();
    selectedMonitor = id;
  }

  async function backToApp(e: MouseEvent) {
    e.stopPropagation();
    if (recording) await stopRecording();
    selectedMonitor = null;
  }

  async function startRecording() {
    if (recording) return;
    const target = captureTarget;
    if (!target) {
      toast(t('toast.selectScreen'), 'problems');
      return;
    }
    try {
      recordingTapped = await invoke<boolean>('start_capture', {
        target,
        fps: captureConfig.fps,
        quality: captureConfig.quality,
        resolution: captureConfig.resolution,
        bitrate: 0,
        mic: micOn,
        micDevice: micInput
      });
      recording = true;
      toast(t('toast.recording'), 'recording', 'ready');
    } catch (e) {
      toast(t('toast.startFailed', { e: String(e) }), 'problems', 'error');
      console.error('start_capture', e);
    }
  }

  async function stopRecording() {
    if (!recording) return;
    recording = false;
    recordingTapped = false;
    try {
      const path = await invoke<string | null>('stop_capture');
      if (path) {
        toast(t('toast.clipSaved'), 'saved', 'saved');
        await refreshLibrary();
      } else {
        toast(t('toast.recStopped'), 'recording', 'info');
      }
    } catch (e) {
      toast(t('toast.stopFailed', { e: String(e) }), 'problems', 'error');
      console.error('stop_capture', e);
    }
  }

  function toggleRecording() {
    if (recording) stopRecording();
    else startRecording();
  }

  function editHotkey() {
    goto('/settings/shortcuts');
  }

  // Guardar clip y grabar los atiende Rust al pulsar el atajo (sin pasar por aquí, que con un
  // juego exigente en primer plano podía tardar segundos). La interfaz solo refleja el resultado.
  $effect(() => {
    const saved = listen('clip-saved', () => refreshLibrary());
    const rec = listen<{ recording: boolean; tapped: boolean; path: string | null }>('recording-changed', (e) => {
      recording = e.payload.recording;
      recordingTapped = e.payload.tapped;
      if (e.payload.path) refreshLibrary();
    });
    return () => {
      saved.then((u) => u());
      rec.then((u) => u());
    };
  });

  // Lo que Rust necesita para grabar con el atajo y para el sonido de guardado.
  $effect(() => {
    const prefs = {
      target: captureTarget,
      fps: captureConfig.fps,
      quality: captureConfig.quality,
      resolution: captureConfig.resolution,
      mic: micOn,
      micDevice: micInput
    };
    invoke('set_record_prefs', { prefs }).catch(() => {});
  });

  $effect(() => {
    invoke('set_save_sound', { gain: gainFor(replaySound.level) }).catch(() => {});
  });

  async function openFlashback() {
    try {
      const w = getCurrentWindow();
      await w.show();
      await w.unminimize();
      await w.setFocus();
    } catch (e) {
      console.error('open window', e);
    }
  }

  type Detected = { name: string; steam_appid: number | null };

  let game = $state('');
  let frame = $state('');
  let frameKey = '';
  let framePending = '';
  // Nombre que se PINTA. `game` es la verdad para la captura y se actualiza al instante; este
  // espera a que el banner esté listo para que el título y el fondo entren a la vez.
  let shown = $state('');

  const gameDisabled = $derived(!!game && gameSettings.isDisabled(game));
  const capSource = $derived(
    selectedMonitor
      ? (activeMonitor?.label ? displaySource(activeMonitor.label) : t('cap.screen'))
      : shown || t('cap.noGame')
  );
  const shownDisabled = $derived(!!shown && gameSettings.isDisabled(shown));

  // En el editor la barra muestra el banner del juego del clip abierto, no el del juego que se
  // está jugando. Las grabaciones de pantalla y los importados no tienen juego y van sin fondo.
  const editorGame = $derived.by(() => {
    const src = editorState.clip?.source ?? '';
    return src && !isScreenSource(src) ? src : '';
  });
  const editorFrame = $derived(editorGame ? gameHero(editorGame) : null);
  $effect(() => {
    if (editorGame) ensureGameHero(editorGame);
  });

  // Objetivo de captura: una pantalla concreta, o la ventana del juego detectado en
  // modo Aplicación. Si es modo Aplicación y NO hay juego (o está deshabilitado), no hay
  // objetivo (null): el usuario debe elegir una pantalla.
  const captureTarget = $derived(selectedMonitor ? selectedMonitor : (game && !gameDisabled) ? 'window' : null);

  async function refresh() {
    try {
      const detected = (await invoke<Detected | null>('detect_game')) ?? null;
      game = detected?.name ?? '';
      if (!detected) {
        shown = '';
        frame = '';
        frameKey = '';
        framePending = '';
        return;
      }
      const key = detected.steam_appid ? `steam:${detected.steam_appid}` : `name:${detected.name}`;
      if (key === frameKey) {
        shown = detected.name;
        return;
      }
      // Una sola petición por juego: al evento le sigue el sondeo, y sin esto el segundo
      // repetiría la descarga mientras la primera sigue en vuelo.
      if (key === framePending) return;
      framePending = key;
      const url = await invoke<string | null>('game_hero', {
        name: detected.name,
        steamAppid: detected.steam_appid
      });
      // Si mientras se descargaba cambiaste de juego, manda el nuevo.
      if (framePending !== key) return;
      // Commit único: el juego sin arte también pasa por aquí, para que su nombre no se quede
      // esperando un banner que no existe.
      frame = artSrc(url) ?? '';
      frameKey = key;
      shown = detected.name;
    } catch {
      // fuera de Tauri (preview en navegador)
    }
  }

  // El watcher nativo avisa al cambiar de juego, así que el fondo y el nombre aparecen al
  // instante en vez de esperar al siguiente sondeo. El intervalo se queda de red de seguridad
  // por si algún cambio no genera evento (y de paso barre procesos mucho menos a menudo).
  $effect(() => {
    refresh();
    loadMonitors();
    loadAudioInputs();
    loadDisabledGames();
    const un = listen('game-changed', () => refresh());
    const id = setInterval(refresh, 10000);
    return () => {
      clearInterval(id);
      un.then((u) => u());
    };
  });

  // Registro de atajos globales: depende de las combinaciones del store y de si se
  // está reasignando alguna (en ese caso se sueltan todos para que el SO no intercepte
  // las teclas). `unregisterAll` al entrar evita callbacks colgados de instancias
  // previas de `tauri dev`; el flag `cancelled` evita que un re-run viejo pise al nuevo.
  $effect(() => {
    const { saveReplay: sr, record: rec, open: op } = hotkeys;
    const paused = capture.active;
    let cancelled = false;
    (async () => {
      try {
        await unregisterAll();
      } catch (e) {
        console.error('unregisterAll', e);
      }
      if (cancelled || paused) return;
      // Cada atajo se registra por separado: en Windows RegisterHotKey falla si la combinación
      // ya la tiene otra app, y antes ese fallo (dentro de un try común) abortaba el registro de
      // los siguientes, tumbando los tres atajos. Aislado, un conflicto solo pierde ese atajo.
      // Guardar y grabar se registran en Rust, que los atiende sin pasar por el webview.
      const failed: string[] = [];
      try {
        const bad = await invoke<string[]>('set_native_hotkeys', { save: sr, record: rec });
        if (bad.includes(sr)) failed.push(`${t('hk.name.saveClip')} (${labelFor(sr)})`);
        if (bad.includes(rec)) failed.push(`${t('hk.name.recording')} (${labelFor(rec)})`);
      } catch (e) {
        console.error('set_native_hotkeys', e);
      }
      const binds: { accel: string; name: string; run: () => void }[] = [
        { accel: op, name: t('hk.name.openFlashback'), run: openFlashback }
      ];
      for (const b of binds) {
        if (cancelled) return;
        try {
          await register(b.accel, (e) => {
            if (e.state === 'Pressed') b.run();
          });
        } catch (e) {
          console.error('register hotkey', b.accel, e);
          failed.push(`${b.name} (${labelFor(b.accel)})`);
        }
      }
      if (!cancelled && failed.length) {
        toast(t('toast.hotkeyInUse', { failed: failed.join(', ') }), 'problems', 'error');
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  // Instant replay: se codifica en segundo plano mientras el toggle esté activo. El
  // efecto reacciona al toggle, a la duración, a la calidad/FPS y a la pantalla objetivo;
  // `lastReplayKey` evita reinicios redundantes cuando nada relevante cambia.
  let lastReplayKey = '';
  $effect(() => {
    const enabled = replay.enabled;
    const seconds = replay.seconds;
    const fps = captureConfig.fps;
    const quality = captureConfig.quality;
    const resolution = captureConfig.resolution;
    const bitrate = 0;
    const target = captureTarget;
    const mic = micOn;
    const micDevice = micInput;
    // En modo Aplicación el objetivo es siempre 'window', pero debe re-armarse al cambiar de juego:
    // se mete la identidad del juego en la key para que cambiar de juego reconstruya la captura
    // contra la nueva ventana (si no, seguiría capturando el juego anterior/minimizado). El relevo
    // launcher → juego con el mismo nombre lo resuelve el backend re-apuntando la captura solo.
    const targetKey = target === 'window' ? `window:${game}` : target;
    const key = enabled && target ? `${targetKey}|${seconds}|${fps}|${quality}|${resolution}|${bitrate}|${mic}|${micDevice}` : 'off';
    if (key === lastReplayKey) return;
    // Solo avisamos "Listo para clipear" al armar el replay desde apagado, no en cada
    // reconfiguración (cambiar calidad/fps reinicia el replay pero no es un evento nuevo).
    const wasOff = lastReplayKey === '' || lastReplayKey === 'off';
    lastReplayKey = key;
    (async () => {
      try {
        if (recording && recordingTapped) await stopRecording();
        await invoke('stop_replay');
        if (key !== 'off') {
          await invoke('start_replay', { target, seconds, fps, quality, resolution, bitrate, mic, micDevice });
          if (wasOff) toast(t('toast.replayReadyHint'), 'ready', 'ready', labelTokens(hotkeys.saveReplay));
        }
      } catch (e) {
        toast(t('toast.replayStartFailed', { e: String(e) }), 'problems', 'error');
        console.error('replay', e);
      }
    })();
  });

  // El backend re-apunta la captura solo (modo Aplicación) cuando la ventana del juego cambia
  // (relevo launcher → juego, recreación al alternar fullscreen). Al reconstruirse contra la
  // nueva ventana emite este evento y volvemos a mostrar el toast "Listo para clipear".
  $effect(() => {
    const un = listen('replay-retargeted', () => {
      toast(t('toast.replayReadyHint'), 'ready', 'ready', labelTokens(hotkeys.saveReplay));
    });
    return () => {
      un.then((u) => u());
    };
  });

  // Chequeo de actualización ~4s tras montar. El popup solo se auto-muestra si la ventana
  // está visible (arranque en bandeja: solo bolita, y al enfocar la ventana se muestra).
  $effect(() => {
    const timer = setTimeout(checkForUpdate, 4000);
    const unlisten = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) maybeAutoShow();
    });
    return () => {
      clearTimeout(timer);
      unlisten.then((u) => u());
    };
  });
</script>

<svelte:window onclick={closeAll} onkeydown={onUpdateKey} oncontextmenu={onContextMenu} />

<div class="app">
  <aside class="sidebar" data-tauri-drag-region>
    <div class="logo" data-tauri-drag-region>
      {#if updater.available}
        <button
          class="logo-btn"
          aria-label={t('upd.badgeLabel')}
          onclick={(e) => {
            e.stopPropagation();
            openUpdatePopup();
          }}
        >
          <img src="/flashback-mono.svg" alt="Flashback" />
          <span class="upd-dot"></span>
        </button>
      {:else}
        <img src="/flashback-mono.svg" alt="Flashback" />
      {/if}
    </div>

    <nav inert={!!editorState.clip}>
      {#each nav as item (item.href)}
        <a
          class="nav-item"
          class:active={isActive(item.href)}
          href={item.href}
          aria-label={t(item.labelKey)}
        >
          <Icon name={item.icon} size={24} />
        </a>
      {/each}
    </nav>

    <a
      class="nav-item games-tab"
      inert={!!editorState.clip}
      class:active={isActive('/juegos')}
      href="/juegos"
      aria-label={t('nav.games')}
    >
      <Icon name="gamepad" size={24} />
    </a>
    <a
      class="nav-item settings-tab"
      inert={!!editorState.clip}
      class:active={isActive('/settings')}
      class:spin={gearSpin}
      onmouseenter={() => (gearSpin = true)}
      onanimationend={() => (gearSpin = false)}
      href="/settings"
      aria-label={t('nav.settings')}
    >
      <Icon name="settings-fill" size={24} />
    </a>
  </aside>

  <div class="main">
    <header class="topbar" data-tauri-drag-region>
      <div class="capture-target">
        <button
          class="capturing"
          class:idle={!selectedMonitor && !shown && !editorState.clip}
          class:screen={!!selectedMonitor}
          class:rec={recording}
          class:open={pickerOpen}
          onclick={togglePicker}
          aria-haspopup="menu"
          aria-expanded={pickerOpen}
        >
          {#if editorState.clip}
            {#key editorFrame}
              {#if editorFrame}
                <span class="cap-frame" style:background-image={`url("${editorFrame}")`}></span>
              {/if}
            {/key}
            <span class="cap-icon" style:background={editorFrame ? 'transparent' : null}>
              <Icon name="editor" size={20} />
            </span>
            <span class="cap-text">
              <span class="cap-label">{t('cap.inEditor')}</span>
              <span class="cap-proc">
                <span class="marq" use:marquee={editorState.clip.title}
                  ><span class="marq-main">{editorState.clip.title}</span></span
                >
              </span>
            </span>
          {:else if selectedMonitor}
            <span class="cap-icon">
              {#if recording}<span class="rec-dot"></span>{:else}<Icon name="monitor" size={20} />{/if}
            </span>
          {:else}
            <!-- key: al cambiar de banner el elemento se recrea y la animación vuelve a correr. -->
            {#key frame}
              {#if frame}
                <span class="cap-frame" style:background-image={`url("${frame}")`}></span>
              {/if}
            {/key}
            <span class="cap-icon" style="background: transparent; color: {shown ? 'var(--bright)' : 'var(--text-2)'}">
              <Icon name="console" size={20} />
            </span>
          {/if}
          {#if !editorState.clip}
            <span class="cap-text">
              <span class="cap-label">
                {#if selectedMonitor}
                  {recording ? t('cap.recordingScreen') : t('cap.screenReady')}
                {:else if shownDisabled}
                  {t('cap.captureDisabled')}
                {:else}
                  {shown ? t('cap.capturingClips') : t('cap.idle')}
                {/if}
              </span>
              <span class="cap-proc">
                <span class="marq" use:marquee={capSource}
                  ><span class="marq-main">{capSource}</span></span
                >
              </span>
            </span>
          {/if}
        </button>

        {#if !editorState.clip && pickerOpen}
          <div class="cap-menu" role="menu" use:flip>
            <button class="cap-opt" class:on={!selectedMonitor} role="menuitem" onclick={(e) => backToApp(e)}>
              <span class="opt-ico"><Icon name="console" size={21} /></span>
              <span class="opt-text">
                <span class="opt-title">{t('cap.application')}</span>
                <span class="opt-sub">{game || t('cap.noGame')}</span>
              </span>
              {#if !selectedMonitor}<span class="opt-check"><Icon name="check" size={15} sw={2.2} /></span>{/if}
            </button>

            <button
              class="cap-opt mic-opt"
              class:on={micOn}
              role="menuitemcheckbox"
              aria-checked={micOn}
              onclick={(e) => {
                e.stopPropagation();
                micOn = !micOn;
                setMic(micOn);
              }}
            >
              <span class="opt-ico"><Icon name="mic" size={21} /></span>
              <span class="mic-label">
                {t('cap.micCapture')}
                <span class="help" aria-label={t('cap.whatOption')}>
                  ?
                  <span class="help-tip" role="tooltip">{t('cap.micTip')}</span>
                </span>
              </span>
              <span class="mic-switch"><span class="mic-knob"></span></span>
            </button>

            <div class="mic-input">
              <div class="mic-dd" class:open={micDDOpen}>
                <button
                  class="mic-trigger"
                  aria-haspopup="listbox"
                  aria-expanded={micDDOpen}
                  aria-label={t('cap.micInput')}
                  onclick={toggleMicDD}
                >
                  <span class="mic-value">{micName}</span>
                  <span class="mic-chev"><Icon name="chevron-down" size={13} sw={2} /></span>
                </button>
                {#if micDDOpen}
                  <div class="mic-list" role="listbox" use:flip>
                    {#each audioInputs as inp (inp.id)}
                      <button
                        class="mic-item"
                        class:on={micInput === inp.id}
                        role="option"
                        aria-selected={micInput === inp.id}
                        onclick={(e) => pickMic(e, inp.id)}
                      >
                        {inp.name}
                        <span class="mic-check"><Icon name="check" size={13} sw={2.2} /></span>
                      </button>
                    {/each}
                    {#if audioInputs.length === 0}
                      <button class="mic-item" disabled>{t('cap.noMics')}</button>
                    {/if}
                  </div>
                {/if}
              </div>
            </div>

            <div class="cap-sep"></div>
            <span class="cap-group">{t('cap.screens')}</span>

            <div class="screen-grid">
              {#each monitors as m (m.id)}
                <button
                  class="screen-card"
                  class:on={selectedMonitor === m.id}
                  role="menuitem"
                  onclick={(e) => selectMonitor(e, m.id)}
                >
                  <span class="screen-thumb" style:background-image={m.thumb ? `url(${m.thumb})` : 'none'}>
                    {#if !m.thumb}<Icon name="monitor" size={22} />{/if}
                    {#if selectedMonitor === m.id}
                      <span class="screen-check"><Icon name="check" size={14} sw={2.4} /></span>
                    {/if}
                  </span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      {#if !editorState.clip}
        <div class="qset" class:open={settingsOpen}>
          <button
            class="qset-trigger mono"
            aria-haspopup="true"
            aria-expanded={settingsOpen}
            onclick={toggleSettings}
          >
            <span class="qset-sum">
              {#each summaryParts as part, i (part.key)}
                {#if i > 0}<span class="qset-dot">|</span>{/if}
                <span class="qpart qpart-{part.key}">{part.text}</span>
              {/each}
            </span>
            <span class="chev"><Icon name="chevron-down" size={11} sw={2} /></span>
          </button>

          {#if settingsOpen}
            <div class="qset-menu" role="menu" use:flip>
              {#each settingRows as row (row.key)}
                <div class="qrow">
                  <span class="qtitle">{row.title}</span>
                  <div class="qdd" class:open={openRow === row.key}>
                    <button
                      class="qdd-trigger"
                      aria-haspopup="listbox"
                      aria-expanded={openRow === row.key}
                      aria-label={row.title}
                      onclick={(e) => toggleRow(e, row.key)}
                    >
                      <span class="qdd-value">{row.value}</span>
                      <span class="qdd-chev"><Icon name="chevron-down" size={13} sw={2} /></span>
                    </button>
                    {#if openRow === row.key}
                      <div class="qdd-list" role="listbox" use:flip>
                        {#each row.options as opt (opt.label)}
                          <button
                            class="qdd-item"
                            class:on={opt.label === row.value}
                            role="option"
                            aria-selected={opt.label === row.value}
                            onclick={(e) => pickRow(e, row.key, opt.raw)}
                          >
                            {opt.label}
                            <span class="qdd-check"><Icon name="check" size={13} sw={2.2} /></span>
                          </button>
                        {/each}
                      </div>
                    {/if}
                  </div>
                </div>
              {/each}

              <div class="qest">
                <span class="qest-label">{t('cap.estSize')}</span>
                <span class="qest-right">
                  <span class="help" aria-label={t('cap.aboutEstSize')}>
                    ?
                    <span class="help-tip" role="tooltip">{t('cap.estSizeTip')}</span>
                  </span>
                  <span class="qest-val mono">{estSize}</span>
                </span>
              </div>
            </div>
          {/if}
        </div>

        <div class="quick">
          <span class="pill combo recpill mono" class:on={recording}>
            <button class="seg rec-seg" onclick={toggleRecording}>
              <span class="rec-ico"><Icon name={recording ? 'pause' : 'play-fill'} size={15} /></span>
              <span class="rec-label">{recording ? t('cap.stopRec') : t('cap.startRec')}</span>
            </button>
            <span class="sep">|</span>
            <button class="seg rec-hotkey" title={t('cap.editRecHotkey')} onclick={editHotkey}>
              {labelFor(hotkeys.record)}
            </button>
          </span>
        </div>
      {:else}<div style="flex:1"></div>
      {/if}

      <div class="winctl"><WindowControls /></div>
    </header>

    <div class="content" class:behind={!!editorState.clip} bind:this={contentEl}>
      {@render children()}
    </div>
  </div>
</div>

{#if editorState.clip}
  {#key editorState.clip.id}
    <Editor />
  {/key}
{/if}

<ShareDialog />
<ConfirmDialog />
<PlaylistDialog />
<ClipEditDialog />

{#if updater.popupOpen && updater.info}
  <div
    class="upd-overlay"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) closeUpdatePopup();
    }}
  >
    <div
      class="upd-modal"
      role="dialog"
      aria-modal="true"
      aria-label={t('upd.title')}
      tabindex="-1"
      use:focusOnMount
    >
      <h2 class="modal-title">{t('upd.title')}</h2>
      <p class="upd-ver">{t('upd.version', { v: updater.info.version })}</p>
      {#if updater.info.notes}<p class="upd-notes">{updater.info.notes}</p>{/if}
      {#if updater.installing}
        <div class="upd-track"><div class="upd-fill" style:width={`${updater.progress}%`}></div></div>
        <p class="upd-status">{t('upd.installing')}</p>
      {:else}
        <div class="upd-actions">
          <button class="upd-btn ghost" onclick={closeUpdatePopup}>{t('upd.cancel')}</button>
          <button class="upd-btn primary" onclick={installUpdate}>{t('upd.update')}</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .app {
    position: relative;
    z-index: 1;
    display: flex;
    height: 100vh;
  }

  .sidebar {
    width: var(--sidebar-w);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 0 0 14px;
    background: var(--base);
  }
  .logo {
    display: grid;
    place-items: center;
    width: 100%;
    height: var(--topbar-h);
    margin-bottom: 8px;
  }
  .logo img {
    display: block;
    width: 30px;
    height: 30px;
    pointer-events: none;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
    align-items: center;
  }
  .nav-item {
    width: 52px;
    height: 52px;
    display: grid;
    place-items: center;
    border-radius: var(--r-md);
    color: var(--text-2);
    position: relative;
    transition: color 0.16s ease, background 0.16s ease;
  }
  .nav-item:hover {
    color: var(--text-1);
    background: var(--bg-2);
  }
  .nav-item.active {
    color: var(--text-0);
    background: var(--bg-2);
  }
  .nav-item.active::before {
    content: '';
    position: absolute;
    left: -12px;
    top: 50%;
    transform: translateY(-50%);
    width: 3px;
    height: 22px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }
  .games-tab {
    margin-top: auto;
  }
  .settings-tab {
    margin-top: 6px;
  }
  /* Animación (no transición) disparada por una clase que se quita en animationend: la vuelta
     siempre se completa aunque el ratón salga antes. */
  .settings-tab.spin :global(svg) {
    animation: gear-spin 1.8s cubic-bezier(0.45, 0.45, 0.35, 1);
  }
  @keyframes gear-spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .settings-tab.spin :global(svg) {
      animation-duration: 1ms;
    }
  }

  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .topbar {
    height: var(--topbar-h);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 18px;
    background: var(--base);
  }

  .capture-target {
    position: relative;
    align-self: stretch;
    display: flex;
  }
  .capturing {
    position: relative;
    align-self: stretch;
    display: flex;
    align-items: center;
    min-width: 240px;
    max-width: 360px;
    padding: 0 16px;
    overflow: hidden;
    text-align: left;
    border-radius: var(--r-sm);
  }
  .cap-icon {
    position: relative;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    margin: 0 11px 0 -22px;
    border-radius: var(--r-sm);
    color: var(--bright);
    background: var(--base);
    flex-shrink: 0;
  }
  .capturing.rec .cap-icon {
    background: transparent;
  }
  .rec-dot {
    width: 11px;
    height: 11px;
    border-radius: 999px;
    background: var(--rec);
    box-shadow: 0 0 0 0 var(--rec-glow);
    animation: rec-pulse 1.4s ease-out infinite;
  }
  @keyframes rec-pulse {
    0% {
      box-shadow: 0 0 0 0 var(--rec-glow);
    }
    70% {
      box-shadow: 0 0 0 7px transparent;
    }
    100% {
      box-shadow: 0 0 0 0 transparent;
    }
  }
  .cap-frame {
    position: absolute;
    inset: 0;
    z-index: 0;
    background-size: cover;
    background-position: center;
    filter: blur(1px) brightness(0.7);
    mask-image: linear-gradient(to right, transparent 0%, #000 14%, #000 72%, transparent 100%);
    animation: frame-in 0.34s ease-out both;
  }
  @keyframes frame-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .cap-frame {
      animation: none;
    }
  }
  .cap-text {
    position: relative;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
    /* El desvanecido de los bordes es para el título que se desplaza. Va en px y con un relleno
       igual de ancho: en % dependía del ancho de la caja y se comía el trazo de la primera letra.
       El margen negativo compensa el relleno para que el texto no se mueva. */
    padding: 0 6px;
    margin-left: -10px;
    overflow: hidden;
    mask-image: linear-gradient(to right, transparent 0, #000 6px, #000 calc(100% - 6px), transparent 100%);
    -webkit-mask-image: linear-gradient(to right, transparent 0, #000 6px, #000 calc(100% - 6px), transparent 100%);
  }
  .cap-label {
    font-size: 12px;
    line-height: 1;
    color: var(--text-1);
  }
  .cap-proc {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 16px;
    font-weight: 600;
    line-height: 1.2;
    color: var(--text-0);
  }
  .capturing.idle .cap-proc {
    color: var(--text-2);
    font-weight: 500;
  }
  /* Sobre el arte del juego el texto se perdía en las zonas claras del banner: una sombra corta
     lo separa de la imagen. Va en todos los estados para que la barra se lea igual con o sin
     arte detrás. */
  .cap-text {
    text-shadow:
      0 1px 2px rgba(0, 0, 0, 0.85),
      0 0 12px rgba(0, 0, 0, 0.55);
  }
  .marq {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  /* inline-block: las transformaciones no se aplican a elementos inline no reemplazados */
  .marq-main {
    display: inline-block;
  }
  .marq:global(.scroll) {
    text-overflow: clip;
  }
  .marq:global(.scroll) .marq-main {
    animation: marquee var(--marq-dur, 6s) ease-in-out infinite;
  }
  @keyframes marquee {
    0%, 10% { transform: translateX(0); opacity: 1; }
    40%, 48% { transform: translateX(var(--marq)); opacity: 1; }
    54% { transform: translateX(var(--marq)); opacity: 0; }
    55% { transform: translateX(0); opacity: 0; }
    61%, 100% { transform: translateX(0); opacity: 1; }
  }

  .cap-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: auto;
    transform-origin: top left;
    width: 420px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 8px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    z-index: 50;
    animation: cap-in 0.14s ease-out;
  }
  @keyframes cap-in {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .cap-menu {
      animation: none;
    }
  }
  .cap-opt {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 9px 10px;
    border-radius: 7px;
    color: var(--text-1);
    text-align: left;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .cap-opt:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .cap-opt.on {
    color: var(--text-0);
  }
  .opt-ico {
    display: grid;
    place-items: center;
    width: 24px;
    color: var(--text-3);
    flex-shrink: 0;
  }
  .cap-opt.on .opt-ico {
    color: var(--bright);
  }
  .screen-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    padding: 2px;
  }
  .screen-card {
    display: flex;
    padding: 3px;
    border-radius: 8px;
  }
  .screen-thumb {
    position: relative;
    display: grid;
    place-items: center;
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: 6px;
    background-color: var(--bg-3);
    background-size: cover;
    background-position: center;
    color: var(--text-3);
    border: 2px solid var(--line);
    overflow: hidden;
    transition: border-color 0.12s ease;
  }
  .screen-card:hover .screen-thumb,
  .screen-card.on .screen-thumb {
    border-color: var(--bright);
  }
  .screen-check {
    position: absolute;
    top: 6px;
    right: 6px;
    display: grid;
    place-items: center;
    width: 23px;
    height: 23px;
    border-radius: 999px;
    color: var(--bg-1);
    background: var(--bright);
    box-shadow: 0 2px 7px rgba(0, 0, 0, 0.35);
  }
  .opt-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .opt-title {
    font-size: 13px;
    line-height: 1.1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .opt-sub {
    font-size: 11px;
    line-height: 1;
    color: var(--text-3);
  }
  .opt-check {
    display: grid;
    place-items: center;
    color: var(--bright);
    flex-shrink: 0;
  }
  .cap-sep {
    height: 1px;
    margin: 5px 6px;
    background: var(--line);
  }
  .cap-group {
    padding: 2px 10px 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    text-align: center;
    color: var(--text-1);
  }

  .mic-label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    font-size: 13px;
    line-height: 1.15;
  }
  .help {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 15px;
    height: 15px;
    margin-left: 7px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-2);
    background: var(--bg-3);
    cursor: help;
    flex-shrink: 0;
  }
  .help:hover {
    color: var(--on-accent);
    background: var(--bright);
  }
  .help-tip {
    position: absolute;
    top: calc(100% + 7px);
    left: 50%;
    transform: translateX(-50%);
    width: 214px;
    padding: 8px 10px;
    font-size: 11.5px;
    font-weight: 400;
    line-height: 1.35;
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
  .help:hover .help-tip {
    opacity: 1;
    visibility: visible;
  }

  .mic-input {
    margin-top: 2px;
    padding: 0 8px 2px;
  }
  .mic-dd {
    position: relative;
  }
  .mic-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    height: 30px;
    padding: 6px;
    font-size: 11px;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: 4px;
    cursor: pointer;
    text-align: left;
    transition: border-color 0.14s ease;
  }
  .mic-trigger:hover,
  .mic-dd.open .mic-trigger {
    border-color: var(--line-strong);
  }
  .mic-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mic-chev {
    display: inline-flex;
    color: var(--text-3);
    flex-shrink: 0;
    transition: transform 0.15s ease;
  }
  .mic-dd.open .mic-chev {
    transform: rotate(180deg);
  }
  .mic-list {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    box-shadow: var(--shadow-pop);
    z-index: 70;
  }
  .mic-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 9px;
    font-size: 11px;
    border-radius: 6px;
    color: var(--text-1);
    text-align: left;
    white-space: nowrap;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .mic-item:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .mic-item.on {
    color: var(--bright);
  }
  .mic-item .mic-check {
    opacity: 0;
    flex-shrink: 0;
    color: var(--bright);
  }
  .mic-item.on .mic-check {
    opacity: 1;
  }

  .mic-switch {
    flex-shrink: 0;
    width: 36px;
    height: 20px;
    border-radius: 999px;
    background: var(--bg-3);
    border: 1px solid var(--line);
    padding: 2px;
    transition: background 0.18s ease, border-color 0.18s ease;
  }
  .mic-knob {
    display: block;
    width: 14px;
    height: 14px;
    border-radius: 999px;
    background: var(--text-2);
    transition: transform 0.18s ease, background 0.18s ease;
  }
  .mic-opt.on .mic-switch {
    background: var(--accent);
    border-color: transparent;
  }
  .mic-opt.on .mic-knob {
    transform: translateX(16px);
    background: var(--on-accent);
  }

  .quick {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 7px;
    flex-shrink: 0;
  }
  .pill {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    font-size: 11.5px;
    color: var(--text-1);
    background: var(--field);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    white-space: nowrap;
  }
  .pill.combo {
    gap: 0;
    padding: 0;
    font-size: 13.5px;
  }
  .pill.combo .seg {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 9px 14px;
    background: none;
    border: 0;
    color: var(--text-1);
    font: inherit;
    white-space: nowrap;
  }
  .pill.combo button.seg {
    cursor: pointer;
    transition: color 0.15s ease;
  }
  .pill.combo button.seg:hover {
    color: var(--text-0);
  }
  .pill.combo .sep {
    color: var(--line-strong);
    padding: 0;
    user-select: none;
  }

  .qset {
    position: relative;
    display: inline-flex;
  }
  .qset-trigger {
    display: inline-flex;
    align-items: center;
    gap: 12px;
    padding: 10px 18px;
    font-size: 13.5px;
    color: var(--text-1);
    background: var(--field);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    cursor: pointer;
    white-space: nowrap;
    transition: color 0.15s ease, border-color 0.15s ease;
  }
  .qset-trigger:hover,
  .qset.open .qset-trigger {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .qset-sum {
    display: inline-flex;
    align-items: center;
  }
  .qpart {
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
  }
  .qpart-dur {
    width: 42px;
  }
  .qpart-qual {
    width: 46px;
  }
  .qpart-res {
    width: 46px;
  }
  .qpart-fps {
    width: 58px;
  }
  .qset-dot {
    margin: 0 14px;
    color: var(--line-strong);
    user-select: none;
  }
  .qset-trigger .chev {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--text-3);
    transition: transform 0.15s ease;
  }
  .qset.open .qset-trigger .chev {
    transform: rotate(180deg);
  }
  .qset-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    transform-origin: top center;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 11px;
    padding: 13px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-pop);
    z-index: 60;
    animation: qset-in 0.14s ease-out;
  }
  @keyframes qset-in {
    from {
      opacity: 0;
      transform: translateX(-50%) translateY(-6px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateX(-50%) translateY(0) scale(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .qset-menu {
      animation: none;
    }
  }
  .qrow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .qtitle {
    font-size: 14.5px;
    font-weight: 560;
    color: var(--text-0);
  }
  .qdd {
    position: relative;
    flex-shrink: 0;
  }
  .qdd-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-width: 118px;
    height: 34px;
    padding: 0 12px;
    font-size: 13px;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    cursor: pointer;
    text-align: left;
    transition: border-color 0.14s ease;
  }
  .qdd-trigger:hover,
  .qdd.open .qdd-trigger {
    border-color: var(--line-strong);
  }
  .qdd-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .qdd-chev {
    display: inline-flex;
    color: var(--text-3);
    flex-shrink: 0;
    transition: transform 0.15s ease;
  }
  .qdd.open .qdd-chev {
    transform: rotate(180deg);
  }
  .qdd-list {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    box-shadow: var(--shadow-pop);
    z-index: 70;
  }
  .qdd-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    font-size: 12.5px;
    border-radius: 6px;
    color: var(--text-1);
    text-align: left;
    white-space: nowrap;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .qdd-item:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .qdd-item.on {
    color: var(--bright);
  }
  .qdd-item .qdd-check {
    opacity: 0;
    flex-shrink: 0;
    color: var(--bright);
  }
  .qdd-item.on .qdd-check {
    opacity: 1;
  }
  .qest {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 1px;
    padding-top: 11px;
    border-top: 1px solid var(--line);
  }
  .qest-label {
    font-size: 12px;
    color: var(--text-2);
  }
  .qest-right {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .qest-val {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-0);
  }

  .recpill {
    background: var(--field);
    transition: background 0.16s ease, border-color 0.16s ease;
  }
  .recpill .rec-seg {
    color: var(--text-0);
  }
  .recpill .rec-ico {
    display: inline-flex;
    align-items: center;
  }
  .recpill.on {
    background: color-mix(in srgb, var(--rec) 16%, transparent);
    border-color: color-mix(in srgb, var(--rec) 55%, transparent);
  }
  .recpill.on .rec-seg {
    color: var(--rec-text);
  }
  .recpill.on .sep {
    color: color-mix(in srgb, var(--rec) 45%, transparent);
  }
  .recpill .rec-hotkey {
    font-size: 11px;
  }

  .winctl {
    align-self: stretch;
    display: flex;
    margin-right: -18px;
  }

  .content {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
    background: var(--base);
  }
  /* Tapada por el editor: sin pintarla, pero montada para conservar scroll y estado. También la
     saca del orden de tabulación. */
  .content.behind {
    visibility: hidden;
  }

  .logo-btn {
    position: relative;
    display: grid;
    place-items: center;
    background: none;
    border: 0;
    padding: 0;
    cursor: pointer;
  }
  .upd-dot {
    position: absolute;
    top: -2px;
    right: -2px;
    width: 9px;
    height: 9px;
    border-radius: 999px;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--base);
  }

  .upd-overlay {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: grid;
    place-items: center;
    background: var(--scrim);
  }
  .upd-modal {
    width: 380px;
    max-width: calc(100vw - 40px);
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 22px;
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-dialog);
  }
  .upd-ver {
    font-size: 13px;
    color: var(--text-1);
  }
  .upd-notes {
    max-height: 160px;
    overflow-y: auto;
    font-size: 12.5px;
    line-height: 1.4;
    color: var(--text-2);
    white-space: pre-wrap;
  }
  .upd-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
  }
  .upd-btn {
    padding: 9px 16px;
    font-size: 13px;
    border-radius: var(--r-sm);
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }
  .upd-btn.ghost {
    color: var(--text-1);
    background: transparent;
    border: 1px solid var(--line);
  }
  .upd-btn.ghost:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .upd-btn.primary {
    color: var(--on-accent);
    background: var(--accent);
    border: 1px solid transparent;
    font-weight: 560;
  }
  .upd-btn.primary:hover {
    background: var(--accent-deep);
  }
  .upd-track {
    height: 7px;
    margin-top: 6px;
    border-radius: 999px;
    background: var(--bg-3);
    overflow: hidden;
  }
  .upd-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s ease;
  }
  .upd-status {
    font-size: 12px;
    color: var(--text-2);
  }
</style>
