<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import Icon from '$lib/components/Icon.svelte';
  import SettingGroup from '$lib/components/settings/SettingGroup.svelte';
  import SettingRow from '$lib/components/settings/SettingRow.svelte';
  import Switch from '$lib/components/settings/Switch.svelte';
  import { type SeenGame, gameSettings, loadDisabledGames, toggleGameDisabled, fetchSeenGames } from '$lib/games.svelte';
  import { t } from '$lib/i18n.svelte';
  import { initial } from '$lib/source-badge';
  import { artSrc } from '$lib/artwork.svelte';

  type Detected = { name: string; steam_appid: number | null };

  let seenGames = $state<SeenGame[]>([]);
  let currentGame = $state<Detected | null>(null);
  let logos = $state<Record<string, string | null>>({});

  function lastSeenLabel(ts: number): string {
    const diff = Math.floor(Date.now() / 1000 - ts);
    if (diff < 60) return t('time.moment');
    if (diff < 3600) return t('time.minAgo', { n: Math.floor(diff / 60) });
    if (diff < 86400) return t('time.hAgo', { n: Math.floor(diff / 3600) });
    const days = Math.floor(diff / 86400);
    if (days === 1) return t('time.yesterday');
    if (days < 7) return t('time.daysAgo', { n: days });
    if (days < 30) {
      const w = Math.floor(days / 7);
      return t(w > 1 ? 'time.weeksAgo' : 'time.weekAgo', { n: w });
    }
    if (days < 365) {
      const m = Math.floor(days / 30);
      return t(m > 1 ? 'time.monthsAgo' : 'time.monthAgo', { n: m });
    }
    const y = Math.floor(days / 365);
    return t(y > 1 ? 'time.yearsAgo' : 'time.yearAgo', { n: y });
  }

  function logoKey(name: string, steam_appid: number | null): string {
    return steam_appid ? `steam:${steam_appid}` : `name:${name}`;
  }

  async function ensureLogo(name: string, steam_appid: number | null) {
    const key = logoKey(name, steam_appid);
    if (key in logos) return;
    logos[key] = null;
    try {
      const url = await invoke<string | null>('game_icon', { name, steamAppid: steam_appid });
      logos = { ...logos, [key]: artSrc(url) };
    } catch {}
  }

  async function refresh() {
    const [seen, detected] = await Promise.all([
      fetchSeenGames(),
      invoke<Detected | null>('detect_game').catch(() => null)
    ]);
    seenGames = seen;
    currentGame = detected;
    for (const g of seen) ensureLogo(g.name, g.steam_appid);
    if (detected) ensureLogo(detected.name, detected.steam_appid);
  }

  // Mismo aviso que usa la barra superior: un juego nuevo también entra en la lista de detectados.
  $effect(() => {
    loadDisabledGames();
    refresh();
    const un = listen('game-changed', () => refresh());
    return () => {
      un.then((u) => u());
    };
  });

  const otherGames = $derived(currentGame ? seenGames.filter((g) => g.name !== currentGame!.name) : seenGames);
</script>

{#snippet gameIcon(name: string, steam_appid: number | null)}
  {@const logo = logos[logoKey(name, steam_appid)]}
  <span class="game-ico">
    {#if logo}<img src={logo} alt="" />{:else}<span class="ini">{initial(name)}</span>{/if}
  </span>
{/snippet}

{#snippet captureSwitch(name: string)}
  <Switch
    checked={!gameSettings.isDisabled(name)}
    onchange={() => toggleGameDisabled(name)}
    label={t('games.captureAria', { name })}
  />
{/snippet}

<SettingGroup title={t('games.now')}>
  {#if currentGame}
    {@const game = currentGame}
    <SettingRow
      title={game.name}
      desc={gameSettings.isDisabled(game.name) ? t('games.captureDisabled') : t('games.capturingClips')}
      muted={gameSettings.isDisabled(game.name)}
    >
      {#snippet lead()}{@render gameIcon(game.name, game.steam_appid)}{/snippet}
      {@render captureSwitch(game.name)}
    </SettingRow>
  {:else}
    <SettingRow title={t('games.none')} desc={t('games.noneHint')}>
      {#snippet lead()}
        <span class="game-ico placeholder"><Icon name="gamepad" size={22} /></span>
      {/snippet}
      <span class="switch-off" inert><Switch checked={false} label={t('games.none')} /></span>
    </SettingRow>
  {/if}
</SettingGroup>

{#if otherGames.length > 0}
  <SettingGroup title={t('games.detected')}>
    {#each otherGames as g (g.name)}
      {@const disabled = gameSettings.isDisabled(g.name)}
      <SettingRow title={g.name} muted={disabled}>
        {#snippet lead()}{@render gameIcon(g.name, g.steam_appid)}{/snippet}
        {#snippet info()}
          <span class="game-sub">
            <span class="sub-primary">{lastSeenLabel(g.last_seen)}</span>
            <span class="sub-secondary">{disabled ? t('games.captureDisabled') : t('games.captureActive')}</span>
          </span>
        {/snippet}
        {@render captureSwitch(g.name)}
      </SettingRow>
    {/each}
  </SettingGroup>
{/if}

<style>
  .game-ico {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-2);
    border-radius: 10px;
    overflow: hidden;
  }
  /* Mismo hueco que la portada por defecto de una playlist, con el mando en lugar del corazón. */
  .game-ico.placeholder {
    color: var(--text-3);
    background: var(--bg-2);
  }
  .switch-off {
    display: flex;
    opacity: 0.4;
  }
  .game-ico img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  /* Recortada a la altura de la mayúscula para que quede en el centro óptico del hueco. */
  .ini {
    display: block;
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }

  /* Al pasar por encima, la fecha deja paso al estado de la captura. */
  .game-sub {
    position: relative;
    display: block;
    height: 1.25em;
    overflow: hidden;
    font-size: 14px;
    line-height: 1.25;
  }
  .game-sub > span {
    position: absolute;
    inset: 0 auto auto 0;
    width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.25s ease;
  }
  .sub-primary {
    color: var(--text-2);
  }
  .sub-secondary {
    transform: translateY(105%);
    opacity: 0;
    color: var(--text-1);
  }
  :global(.row:hover) .sub-primary {
    transform: translateY(-105%);
    opacity: 0;
  }
  :global(.row:hover) .sub-secondary {
    transform: translateY(0);
    opacity: 1;
  }
  @media (prefers-reduced-motion: reduce) {
    .game-sub > span {
      transition: none;
    }
  }

</style>
