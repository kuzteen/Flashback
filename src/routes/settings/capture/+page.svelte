<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Dropdown from '$lib/components/Dropdown.svelte';
  import SettingGroup from '$lib/components/settings/SettingGroup.svelte';
  import SettingRow from '$lib/components/settings/SettingRow.svelte';
  import Switch from '$lib/components/settings/Switch.svelte';
  import { t } from '$lib/i18n.svelte';
  import { replay, setReplayEnabled, setReplaySeconds, BUFFER_OPTIONS } from '$lib/replay.svelte';
  import {
    captureConfig,
    setFps,
    setQuality,
    setResolution,
    qualityLabel,
    FPS_OPTIONS,
    QUALITY_OPTIONS,
    RES_OPTIONS
  } from '$lib/capture-config.svelte';

  const ENCODER_OPTIONS = ['Auto', 'NVENC', 'AMF', 'Quick Sync', 'Software'] as const;
  type EncoderOption = (typeof ENCODER_OPTIONS)[number];

  const resOptions = RES_OPTIONS.map((o) => ({ label: o.label, value: o.height }));
  const fpsOptions = FPS_OPTIONS.map((o) => ({ label: `${o} fps`, value: o }));
  const qualityOptions = $derived(QUALITY_OPTIONS.map((o) => ({ label: qualityLabel(o.key), value: o.key })));
  const bufferOptions = BUFFER_OPTIONS.map((o) => ({ label: o.label, value: o.seconds }));
  const encoderOptions = ENCODER_OPTIONS.map((o) => ({ label: o, value: o }));

  let encoder = $state<EncoderOption>('Auto');
  invoke<string>('get_encoder')
    .then((e) => {
      if (ENCODER_OPTIONS.includes(e as EncoderOption)) encoder = e as EncoderOption;
    })
    .catch(() => {});

  function setEncoder(opt: EncoderOption) {
    encoder = opt;
    invoke('set_encoder', { enc: opt }).catch(() => {});
  }
</script>

<SettingGroup title={t('settings.group.video')}>
  <SettingRow title={t('settings.resolution')} desc={t('settings.resolution.desc')}>
    <Dropdown value={captureConfig.resolution} options={resOptions} onchange={setResolution} ariaLabel={t('settings.resolution')} />
  </SettingRow>
  <SettingRow title={t('settings.fps')} desc={t('settings.fps.desc')}>
    <Dropdown value={captureConfig.fps} options={fpsOptions} onchange={setFps} ariaLabel={t('settings.fps')} />
  </SettingRow>
  <SettingRow title={t('settings.quality')} desc={t('settings.quality.desc')}>
    <Dropdown value={captureConfig.quality} options={qualityOptions} onchange={setQuality} ariaLabel={t('settings.quality')} />
  </SettingRow>
</SettingGroup>

<SettingGroup title={t('settings.group.replay')}>
  <SettingRow title={t('settings.replayBg')} desc={t('settings.replayBg.desc')}>
    <Switch checked={replay.enabled} onchange={setReplayEnabled} label={t('settings.replayBg')} />
  </SettingRow>
  <SettingRow title={t('settings.bufferLen')} desc={t('settings.bufferLen.desc')} disabled={!replay.enabled}>
    <Dropdown value={replay.seconds} options={bufferOptions} onchange={setReplaySeconds} ariaLabel={t('settings.bufferLen')} />
  </SettingRow>
</SettingGroup>

<SettingGroup title={t('settings.group.advanced')}>
  <SettingRow title={t('settings.encoder')} desc={t('settings.encoder.desc')}>
    <Dropdown value={encoder} options={encoderOptions} onchange={setEncoder} ariaLabel={t('settings.encoder')} />
  </SettingRow>
</SettingGroup>
