<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { untrack } from 'svelte';
  import { artSrc } from '$lib/artwork.svelte';
  import { initial } from '$lib/source-badge';
  import { hoverPill } from '$lib/pill';
  import { flip } from '$lib/flip';
  import Icon from '$lib/components/Icon.svelte';
  import Stepper from '$lib/components/Stepper.svelte';
  import SettingGroup from '$lib/components/settings/SettingGroup.svelte';
  import SettingRow from '$lib/components/settings/SettingRow.svelte';
  import { t } from '$lib/i18n.svelte';
  import { captureConfig, setAudioMode, addAudioApp, removeAudioApp, PRESET_APPS, type AudioApp, type AudioMode } from '$lib/capture-config.svelte';

  let { id, heading = true }: { id?: string; heading?: boolean } = $props();

  // "Juego y apps" (juego, Discord y Spotify) y "Personalizado" solo graban el juego y sus apps.
  // Windows 10 no tiene la API, así que ahí el modo se queda en "Todo el PC".
  type AudioAppInfo = AudioApp & { icon: string | null; active: boolean };
  let appsSupported = $state(true);
  invoke<boolean>('audio_apps_supported')
    .then((v) => (appsSupported = v))
    .catch(() => {});
  const audioModeOptions = $derived<{ label: string; value: AudioMode }[]>([
    { label: t('audio.all'), value: 'all' },
    { label: t('audio.game.preset'), value: 'game' },
    { label: t('audio.apps'), value: 'apps' }
  ]);
  const shownApps = $derived(captureConfig.audioMode === 'game' ? PRESET_APPS : captureConfig.audioApps);

  // El juego detectado ocupa la fila del juego con su nombre y su icono, igual que en Ajustes > Juegos.
  type Detected = { name: string; steam_appid: number | null };
  let game = $state<Detected | null>(null);
  let gameIcon = $state<string | null>(null);

  async function refreshGame() {
    const g = await invoke<Detected | null>('detect_game').catch(() => null);
    if (g?.name === game?.name) return;
    game = g;
    gameIcon = null;
    if (!g) return;
    const url = await invoke<string | null>('game_icon', { name: g.name, steamAppid: g.steam_appid }).catch(() => null);
    if (game?.name === g.name) gameIcon = artSrc(url);
  }

  $effect(() => {
    refreshGame();
    const un = listen('game-changed', () => refreshGame());
    return () => {
      un.then((u) => u());
    };
  });

  const presetIcons: Record<string, string> = {
    'discord.exe': '/audio-app-icons/discord.png',
    'spotify.exe': '/audio-app-icons/spotify.png'
  };
  let appIcons = $state<Record<string, string | null>>({});
  $effect(() => {
    const missing = shownApps.filter((a) => !presetIcons[a.exe.toLowerCase()] && untrack(() => !(a.exe in appIcons)));
    if (missing.length === 0) return;
    for (const a of missing) appIcons[a.exe] = null;
    for (const a of missing) {
      invoke<string | null>('audio_app_icon', { exe: a.exe, path: a.path || null })
        .then((icon) => (appIcons[a.exe] = icon))
        .catch(() => {});
    }
  });

  let appMenuOpen = $state(false);
  let appMenuEl = $state<HTMLElement | null>(null);
  let sessions = $state<AudioAppInfo[]>([]);
  const offered = $derived(
    sessions.filter((s) => !captureConfig.audioApps.some((a) => a.exe.toLowerCase() === s.exe.toLowerCase()))
  );

  function toggleAppMenu() {
    appMenuOpen = !appMenuOpen;
    if (!appMenuOpen) return;
    invoke<AudioAppInfo[]>('audio_sessions')
      .then((list) => (sessions = list))
      .catch(() => {});
  }

  function pickApp(app: AudioAppInfo) {
    appMenuOpen = false;
    appIcons[app.exe] = app.icon;
    addAudioApp({ exe: app.exe, name: app.name, path: app.path });
    invoke('audio_app_icon', { exe: app.exe, path: app.path }).catch(() => {});
  }

  async function browseApp() {
    appMenuOpen = false;
    const app = await invoke<AudioAppInfo | null>('pick_audio_app', { filterName: t('audio.exeFilter') }).catch(() => null);
    if (app) pickApp(app);
  }

  function dismissOnOutside(el: () => HTMLElement | null, close: () => void) {
    const onDown = (e: MouseEvent) => {
      const node = el();
      if (node && !node.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    window.addEventListener('mousedown', onDown, true);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onDown, true);
      window.removeEventListener('keydown', onKey);
    };
  }

  $effect(() => {
    if (appMenuOpen) return dismissOnOutside(() => appMenuEl, () => (appMenuOpen = false));
  });

</script>

{#snippet appIcon(src: string | null | undefined, size: number, exe = '')}
  {@const icon = src ?? presetIcons[exe.toLowerCase()]}
  <span class="app-ico" style:--ico="{size}px">
    {#if icon}<img src={icon} alt="" />{:else}<Icon name="app" size={Math.round(size * 0.6)} />{/if}
  </span>
{/snippet}

<SettingGroup {id} title={heading ? t('settings.group.audio') : undefined}>
  <SettingRow
    title={t('settings.audioSource')}
    desc={appsSupported ? t('settings.audioSource.desc') : t('settings.audioSource.win10')}
    disabled={!appsSupported}
  >
    <Stepper
      value={appsSupported ? captureConfig.audioMode : 'all'}
      options={audioModeOptions}
      onchange={setAudioMode}
      ariaLabel={t('settings.audioSource')}
    />
  </SettingRow>
  {#if appsSupported && captureConfig.audioMode !== 'all'}
    <SettingRow
      title={game ? game.name : t('audio.game')}
      desc={game ? t('audio.game.detected') : t('audio.game.desc')}
      sub
    >
      {#snippet lead()}
        {#if game}
          <span class="app-ico cover" style:--ico="32px">
            {#if gameIcon}<img src={gameIcon} alt="" />{:else}<span class="ini">{initial(game.name)}</span>{/if}
          </span>
        {:else}
          <span class="app-ico placeholder" style:--ico="32px"><Icon name="gamepad" size={18} /></span>
        {/if}
      {/snippet}
    </SettingRow>
    {#each shownApps as app (app.exe)}
      <SettingRow title={app.name} desc={app.exe} sub>
        {#snippet lead()}{@render appIcon(appIcons[app.exe], 32, app.exe)}{/snippet}
        {#if captureConfig.audioMode === 'apps'}
          <button
            class="remove"
            aria-label={t('audio.remove', { name: app.name })}
            title={t('audio.remove', { name: app.name })}
            onclick={() => removeAudioApp(app.exe)}
          >
            <Icon name="close" size={14} sw={2} />
          </button>
        {/if}
      </SettingRow>
    {/each}
    {#if captureConfig.audioMode === 'apps'}
      <SettingRow title={t('audio.add')} desc={t('audio.add.desc')} sub>
        <div class="dd" class:open={appMenuOpen} bind:this={appMenuEl}>
          <button class="dd-trigger" aria-haspopup="menu" aria-expanded={appMenuOpen} onclick={toggleAppMenu}>
            <span class="dd-value">{t('audio.choose')}</span>
            <Icon name="chevron-down" size={12} sw={2} />
          </button>
          {#if appMenuOpen}
            <div class="dd-menu" role="menu" use:flip use:hoverPill={{ selector: '.dd-item', axis: 'y' }}>
              {#each offered as s (s.exe)}
                <button class="dd-item" role="menuitem" onclick={() => pickApp(s)}>
                  {@render appIcon(s.icon, 20, s.exe)}
                  <span class="dd-name">{s.name}</span>
                  {#if s.active}<span class="dd-tag">{t('audio.playing')}</span>{/if}
                </button>
              {/each}
              {#if offered.length === 0}
                <span class="dd-empty">{t('audio.noSessions')}</span>
              {/if}
              <span class="dd-sep"></span>
              <button class="dd-item" role="menuitem" onclick={browseApp}>
                <span class="app-ico" style:--ico="20px"><Icon name="folder-open" size={14} /></span>
                <span class="dd-name">{t('audio.browse')}</span>
              </button>
            </div>
          {/if}
        </div>
      </SettingRow>
    {/if}
  {/if}
</SettingGroup>

<style>
  .dd {
    position: relative;
    width: 260px;
  }
  .dd-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 34px;
    padding: 0 10px 0 12px;
    font-size: 13px;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    text-align: left;
    transition: border-color 0.14s ease;
  }
  .dd-trigger:hover,
  .dd.open .dd-trigger {
    border-color: var(--line-strong);
  }
  .dd-trigger > :global(svg) {
    flex-shrink: 0;
    color: var(--text-2);
    transition: transform 0.2s ease;
  }
  .dd.open .dd-trigger > :global(svg) {
    transform: rotate(180deg);
  }
  .dd-value,
  .dd-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dd-menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    max-height: 60vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    box-shadow: var(--shadow-pop);
    z-index: 70;
  }
  .dd-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    font-size: 13px;
    text-align: left;
    color: var(--text-1);
    border-radius: 6px;
    transition: background 0.13s ease, color 0.13s ease;
  }
  .dd-item:hover {
    color: var(--text-0);
  }
  .dd-menu > :global(.slide-pill) {
    background: var(--bg-3);
    border-radius: 6px;
  }
  .dd-tag {
    flex-shrink: 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .dd-sep {
    height: 1px;
    margin: 4px 2px;
    background: var(--line);
  }
  .app-ico {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: var(--ico);
    height: var(--ico);
    color: var(--text-2);
    border-radius: 6px;
    overflow: hidden;
  }
  .app-ico.placeholder {
    color: var(--text-3);
    background: var(--bg-2);
  }
  .app-ico img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .app-ico.cover img {
    object-fit: cover;
  }
  .ini {
    display: block;
    font-size: 13px;
    font-weight: 600;
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .remove {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: background 0.13s ease, color 0.13s ease;
  }
  .remove:hover {
    color: var(--text-0);
    background: var(--bg-3);
  }
  .dd-empty {
    padding: 7px 8px;
    font-size: 13px;
    color: var(--text-3);
  }

</style>
