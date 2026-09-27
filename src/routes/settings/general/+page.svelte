<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/components/Icon.svelte';
  import Stepper from '$lib/components/Stepper.svelte';
  import SettingGroup from '$lib/components/settings/SettingGroup.svelte';
  import SettingRow from '$lib/components/settings/SettingRow.svelte';
  import Switch from '$lib/components/settings/Switch.svelte';
  import SoundTrimDialog, { type SoundDraft } from '$lib/components/settings/SoundTrimDialog.svelte';
  import { t, getLocale, setLocale, LOCALES, type Locale } from '$lib/i18n.svelte';
  import {
    replaySound,
    setReplaySoundLevel,
    playReplaySound,
    soundLabel,
    SOUND_OPTIONS
  } from '$lib/replay-sound.svelte';

  type ToastPrefs = { enabled: boolean; saved: boolean; ready: boolean; recording: boolean; problems: boolean };
  type ToastTopic = Exclude<keyof ToastPrefs, 'enabled'>;

  const languageOptions = LOCALES.map((l) => ({ label: l.label, value: l.value }));
  const soundOptions = $derived(SOUND_OPTIONS.map((o) => ({ label: soundLabel(o.key), value: o.key })));
  const toastRows: { key: ToastTopic; labelKey: string }[] = [
    { key: 'saved', labelKey: 'settings.toasts.saved' },
    { key: 'ready', labelKey: 'settings.toasts.ready' },
    { key: 'recording', labelKey: 'settings.toasts.recording' },
    { key: 'problems', labelKey: 'settings.toasts.problems' }
  ];

  let discordRpc = $state(false);
  let customSound = $state<string | null>(null);
  let picking = $state(false);
  let trimDraft = $state<SoundDraft | null>(null);
  let soundError = $state(false);
  let autostart = $state(false);
  let toasts = $state<ToastPrefs>({ enabled: true, saved: true, ready: true, recording: true, problems: true });

  invoke<boolean>('get_autostart')
    .then((v) => (autostart = v))
    .catch(() => {});
  invoke<boolean>('get_discord_rpc')
    .then((v) => (discordRpc = v))
    .catch(() => {});
  invoke<string | null>('get_save_sound_name')
    .then((v) => (customSound = v))
    .catch(() => {});
  invoke<ToastPrefs>('get_toast_prefs')
    .then((v) => (toasts = v))
    .catch(() => {});

  function setAutostart(on: boolean) {
    autostart = on;
    invoke('set_autostart', { enabled: on }).catch((e) => {
      console.error('set_autostart', e);
      autostart = !on;
    });
  }

  function setDiscordRpc(on: boolean) {
    discordRpc = on;
    invoke('set_discord_rpc', { enabled: on }).catch(() => {});
  }

  async function pickSound() {
    if (picking) return;
    picking = true;
    soundError = false;
    try {
      const draft = await invoke<SoundDraft | null>('pick_save_sound', { filterName: t('settings.customSound.filter') });
      if (draft) trimDraft = draft;
    } catch (e) {
      console.error('pick_save_sound', e);
      soundError = true;
    } finally {
      picking = false;
    }
  }

  function closeTrim(name: string | null) {
    trimDraft = null;
    if (name) customSound = name;
  }

  function removeSound() {
    soundError = false;
    invoke('clear_save_sound')
      .then(() => (customSound = null))
      .catch((e) => console.error('clear_save_sound', e));
  }

  function openLogs() {
    invoke('open_logs_dir').catch((e) => console.error('open_logs_dir', e));
  }

  function setToast(key: keyof ToastPrefs, on: boolean) {
    toasts[key] = on;
    invoke('set_toast_prefs', { prefs: $state.snapshot(toasts) }).catch(() => {});
  }
</script>

<SettingGroup id="interface" title={t('settings.group.interface')}>
  <SettingRow title={t('settings.language')} desc={t('settings.language.desc')}>
    <Stepper value={getLocale()} options={languageOptions} onchange={(v) => setLocale(v as Locale)} ariaLabel={t('settings.language')} />
  </SettingRow>
</SettingGroup>

<SettingGroup id="notifications" title={t('settings.group.notifications')}>
  <SettingRow title={t('settings.saveSound')} desc={t('settings.saveSound.desc')}>
    <Stepper value={replaySound.level} options={soundOptions} onchange={setReplaySoundLevel} ariaLabel={t('settings.soundVolume')} />
    <button
      class="play-btn"
      aria-label={t('settings.testSound')}
      title={t('settings.testSound')}
      disabled={replaySound.level === 'off'}
      onclick={() => playReplaySound()}
    >
      <Icon name="play-fill" size={18} />
    </button>
  </SettingRow>

  <SettingRow title={t('settings.customSound')} desc={t('settings.customSound.desc')} disabled={replaySound.level === 'off'}>
    {#snippet info()}
      <p class="file mono" class:none={!customSound} title={customSound ?? undefined}>
        {customSound ?? t('settings.customSound.none')}
      </p>
      {#if soundError}<p class="err">{t('settings.customSound.error')}</p>{/if}
    {/snippet}
    {#if customSound}
      <button class="btn" onclick={removeSound}><span class="txt">{t('settings.customSound.remove')}</span></button>
    {/if}
    <button class="btn" onclick={pickSound} disabled={picking}>
      <Icon name="folder-open" size={16} sw={2} /><span class="txt">{t('settings.customSound.pick')}</span>
    </button>
  </SettingRow>

  <SettingRow title={t('settings.toasts')} desc={t('settings.toasts.desc')}>
    <Switch checked={toasts.enabled} onchange={(v) => setToast('enabled', v)} label={t('settings.toasts')} />
  </SettingRow>
  {#if toasts.enabled}
    {#each toastRows as row (row.key)}
      <SettingRow title={t(row.labelKey)} sub>
        <Switch checked={toasts[row.key]} onchange={(v) => setToast(row.key, v)} label={t(row.labelKey)} />
      </SettingRow>
    {/each}
  {/if}
</SettingGroup>

{#if trimDraft}
  <SoundTrimDialog draft={trimDraft} onclose={closeTrim} />
{/if}

<SettingGroup id="integrations" title={t('settings.group.integrations')}>
  <SettingRow title={t('settings.discordRpc')} desc={t('settings.discordRpc.desc')}>
    <Switch checked={discordRpc} onchange={setDiscordRpc} label={t('settings.discordRpc')} />
  </SettingRow>
</SettingGroup>

<SettingGroup id="system" title={t('settings.group.system')}>
  <SettingRow title={t('settings.autostart')} desc={t('settings.autostart.desc')}>
    <Switch checked={autostart} onchange={setAutostart} label={t('settings.autostart')} />
  </SettingRow>
</SettingGroup>

<SettingGroup id="support" title={t('settings.group.support')}>
  <SettingRow title={t('settings.logs')} desc={t('settings.logs.desc')}>
    <button class="btn" onclick={openLogs}><Icon name="folder-open" size={16} sw={2} /><span class="txt">{t('settings.open')}</span></button>
  </SettingRow>
</SettingGroup>

<style>
  .play-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 44px;
    height: 34px;
    color: var(--on-bright);
    background: var(--bright);
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    transition: background 0.15s ease, transform 0.1s ease, opacity 0.15s ease;
  }
  .play-btn:hover:not(:disabled) {
    background: var(--text-1);
  }
  .play-btn:active:not(:disabled) {
    transform: scale(0.96);
  }
  .play-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .file {
    max-width: 320px;
    margin-bottom: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    color: var(--text-1);
  }
  .file.none {
    color: var(--text-3);
  }
  .err {
    margin-bottom: 4px;
    font-size: 12px;
    color: var(--rec-text);
  }
</style>
