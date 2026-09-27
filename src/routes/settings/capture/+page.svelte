<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy } from 'svelte';
  import Icon from '$lib/components/Icon.svelte';
  import { hoverPill } from '$lib/pill';
  import { flip } from '$lib/flip';
  import Stepper from '$lib/components/Stepper.svelte';
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
    setMic,
    setMicDevice,
    setNoiseSuppression,
    setNoiseLevel,
    qualityLabel,
    FPS_OPTIONS,
    QUALITY_OPTIONS,
    RES_OPTIONS
  } from '$lib/capture-config.svelte';


  const resOptions = RES_OPTIONS.map((o) => ({ label: o.label, value: o.height }));
  const fpsOptions = FPS_OPTIONS.map((o) => ({ label: `${o} fps`, value: o }));
  const qualityOptions = $derived(QUALITY_OPTIONS.map((o) => ({ label: qualityLabel(o.key), value: o.key })));
  const bufferOptions = BUFFER_OPTIONS.map((o) => ({ label: o.label, value: o.seconds }));
  // Solo los encoders que existen en este equipo, y Auto dice cuál elegiría. Uno guardado que ya
  // no está (otra GPU) se sigue mostrando para que la elección no cambie sola en pantalla.
  let encoder = $state('Auto');
  let encoderInfo = $state<{ available: string[]; auto: string }>({ available: [], auto: '' });
  const encoderOptions = $derived.by(() => {
    const values = ['Auto', ...encoderInfo.available, 'Software'];
    if (!values.includes(encoder)) values.splice(values.length - 1, 0, encoder);
    return values.map((v) => ({ label: v === 'Auto' && encoderInfo.auto ? `Auto (${encoderInfo.auto})` : v, value: v }));
  });
  invoke<string>('get_encoder')
    .then((e) => (encoder = e))
    .catch(() => {});
  invoke<{ available: string[]; auto: string }>('encoder_options')
    .then((o) => (encoderInfo = o))
    .catch(() => {});

  function setEncoder(opt: string) {
    encoder = opt;
    invoke('set_encoder', { enc: opt }).catch(() => {});
  }

  type AudioInput = { id: string; name: string };
  let audioInputs = $state<AudioInput[]>([]);
  let micMenuOpen = $state(false);
  let micMenuEl = $state<HTMLElement | null>(null);
  const micName = $derived(audioInputs.find((d) => d.id === captureConfig.micDevice)?.name ?? t('cap.noMics'));

  invoke<AudioInput[]>('list_audio_inputs')
    .then((list) => {
      audioInputs = list;
      if (!list.some((d) => d.id === captureConfig.micDevice) && list[0]) setMicDevice(list[0].id);
    })
    .catch(() => {});

  $effect(() => {
    if (!micMenuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (micMenuEl && !micMenuEl.contains(e.target as Node)) micMenuOpen = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') micMenuOpen = false;
    };
    window.addEventListener('mousedown', onDown, true);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onDown, true);
      window.removeEventListener('keydown', onKey);
    };
  });

  function pickMic(id: string) {
    micMenuOpen = false;
    if (id === captureConfig.micDevice) return;
    discardTest();
    setMicDevice(id);
  }

  // Prueba de voz: graba hasta que se pausa y suena con el nivel del deslizador. La toma queda
  // en Rust para volver a escucharla con otro nivel sin repetir la frase.
  const MAX_TEST_MS = 30_000;
  let test = $state<'idle' | 'recording' | 'playing'>('idle');
  let hasTake = $state(false);
  let elapsed = $state(0);
  let timer = 0;

  const testLevel = () => (captureConfig.noiseSuppression ? captureConfig.noiseLevel : 0);
  const testLabel = $derived(
    test === 'recording' ? t('settings.micTest.listen') : test === 'playing' ? t('settings.micTest.stop') : t('settings.micTest.record')
  );

  function clearTimer() {
    clearInterval(timer);
    clearTimeout(timer);
  }

  async function startTest() {
    try {
      await invoke('mic_test_record', { device: captureConfig.micDevice });
    } catch (e) {
      console.error('mic_test_record', e);
      return;
    }
    test = 'recording';
    const t0 = performance.now();
    elapsed = 0;
    clearTimer();
    timer = window.setInterval(() => {
      elapsed = performance.now() - t0;
      if (elapsed >= MAX_TEST_MS) play();
    }, 200);
  }

  async function play() {
    clearTimer();
    const ms = await invoke<number>('mic_test_play', { level: testLevel() }).catch(() => 0);
    hasTake = ms > 0;
    if (!hasTake) {
      test = 'idle';
      return;
    }
    test = 'playing';
    timer = window.setTimeout(() => (test = 'idle'), ms + 150);
  }

  function stopPlayback() {
    clearTimer();
    invoke('mic_test_stop').catch(() => {});
    test = 'idle';
  }

  function onTestClick() {
    if (test === 'recording') play();
    else if (test === 'playing') stopPlayback();
    else startTest();
  }

  function discardTest() {
    clearTimer();
    test = 'idle';
    hasTake = false;
    invoke('mic_test_discard').catch(() => {});
  }

  function commitLevel(level: number) {
    setNoiseLevel(level);
    if (test === 'playing') play();
  }

  onDestroy(discardTest);

  const fmtSecs = (ms: number) => `0:${String(Math.floor(ms / 1000)).padStart(2, '0')}`;
</script>

<SettingGroup id="replay" title={t('settings.group.replay')}>
  <SettingRow title={t('settings.replayBg')} desc={t('settings.replayBg.desc')}>
    <Switch checked={replay.enabled} onchange={setReplayEnabled} label={t('settings.replayBg')} />
  </SettingRow>
  <SettingRow title={t('settings.bufferLen')} desc={t('settings.bufferLen.desc')} disabled={!replay.enabled}>
    <Stepper value={replay.seconds} options={bufferOptions} onchange={setReplaySeconds} ariaLabel={t('settings.bufferLen')} />
  </SettingRow>
</SettingGroup>

<SettingGroup id="video" title={t('settings.group.video')}>
  <SettingRow title={t('settings.quality')} desc={t('settings.quality.desc')}>
    <Stepper value={captureConfig.quality} options={qualityOptions} onchange={setQuality} ariaLabel={t('settings.quality')} />
  </SettingRow>
  <SettingRow title={t('settings.resolution')} desc={t('settings.resolution.desc')}>
    <Stepper value={captureConfig.resolution} options={resOptions} onchange={setResolution} ariaLabel={t('settings.resolution')} />
  </SettingRow>
  <SettingRow title={t('settings.fps')} desc={t('settings.fps.desc')}>
    <Stepper value={captureConfig.fps} options={fpsOptions} onchange={setFps} ariaLabel={t('settings.fps')} />
  </SettingRow>
</SettingGroup>

<SettingGroup id="mic" title={t('settings.group.mic')}>
  <SettingRow title={t('settings.mic')} desc={t('settings.mic.desc')}>
    <Switch checked={captureConfig.mic} onchange={setMic} label={t('settings.mic')} />
  </SettingRow>
  <SettingRow title={t('settings.micInput')} sub disabled={!captureConfig.mic}>
    <div class="dd" class:open={micMenuOpen} bind:this={micMenuEl}>
      <button
        class="dd-trigger"
        aria-haspopup="listbox"
        aria-expanded={micMenuOpen}
        aria-label={t('settings.micInput')}
        onclick={() => (micMenuOpen = !micMenuOpen)}
      >
        <span class="dd-value">{micName}</span>
        <Icon name="chevron-down" size={12} sw={2} />
      </button>
      {#if micMenuOpen}
        <div class="dd-menu" role="listbox" use:flip use:hoverPill={{ selector: '.dd-item', axis: 'y' }}>
          {#each audioInputs as inp (inp.id)}
            <button
              class="dd-item"
              class:on={captureConfig.micDevice === inp.id}
              role="option"
              aria-selected={captureConfig.micDevice === inp.id}
              onclick={() => pickMic(inp.id)}
            >
              <span class="dd-name">{inp.name}</span>
              <span class="dd-check"><Icon name="check" size={13} sw={2.2} /></span>
            </button>
          {/each}
          {#if audioInputs.length === 0}
            <span class="dd-empty">{t('cap.noMics')}</span>
          {/if}
        </div>
      {/if}
    </div>
  </SettingRow>
  <SettingRow title={t('settings.noise')} desc={t('settings.noise.desc')} disabled={!captureConfig.mic}>
    <Switch checked={captureConfig.noiseSuppression} onchange={setNoiseSuppression} label={t('settings.noise')} />
  </SettingRow>
  {#if captureConfig.noiseSuppression}
    <SettingRow title={t('settings.noiseLevel')} sub disabled={!captureConfig.mic}>
      <div class="level">
        <input
          class="fader"
          type="range"
          min="0"
          max="100"
          step="1"
          aria-label={t('settings.noiseLevel')}
          value={captureConfig.noiseLevel}
          oninput={(e) => (captureConfig.noiseLevel = Number(e.currentTarget.value))}
          onchange={(e) => commitLevel(Number(e.currentTarget.value))}
        />
        <span class="level-value">{captureConfig.noiseLevel}%</span>
      </div>
    </SettingRow>
  {/if}
  <SettingRow title={t('settings.micTest')} desc={t('settings.micTest.desc')} sub disabled={!captureConfig.mic || audioInputs.length === 0}>
    {#if hasTake && test === 'idle'}
      <button class="btn" onclick={play}><span class="txt">{t('settings.micTest.again')}</span></button>
    {/if}
    <button class="test-btn" class:recording={test === 'recording'} aria-label={testLabel} title={testLabel} onclick={onTestClick}>
      {#if test === 'recording'}
        <span class="rec-dot"></span><span class="txt">{fmtSecs(elapsed)}</span><Icon name="pause" size={15} />
      {:else if test === 'playing'}
        <span class="txt">{t('settings.micTest.playing')}</span><span class="stop-sq"></span>
      {:else}
        <Icon name="play-fill" size={17} />
      {/if}
    </button>
  </SettingRow>
</SettingGroup>

<SettingGroup id="advanced" title={t('settings.group.advanced')}>
  <SettingRow title={t('settings.encoder')} desc={t('settings.encoder.desc')}>
    <Stepper value={encoder} options={encoderOptions} onchange={setEncoder} ariaLabel={t('settings.encoder')} />
  </SettingRow>
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
  .dd-item:hover,
  .dd-item.on {
    color: var(--text-0);
  }
  .dd-menu > :global(.slide-pill) {
    background: var(--bg-3);
    border-radius: 6px;
  }
  .dd-check {
    display: inline-flex;
    flex-shrink: 0;
    opacity: 0;
    color: var(--bright);
  }
  .dd-item.on .dd-check {
    opacity: 1;
  }
  .dd-empty {
    padding: 7px 8px;
    font-size: 13px;
    color: var(--text-3);
  }

  .level {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 260px;
  }
  .level-value {
    width: 38px;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    text-align: right;
    color: var(--text-1);
  }

  .test-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    flex-shrink: 0;
    min-width: 44px;
    height: 34px;
    padding: 0 12px;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
    color: var(--on-bright);
    background: var(--bright);
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    transition: background 0.15s ease, transform 0.1s ease;
  }
  .test-btn:hover {
    background: var(--text-1);
  }
  .test-btn:active {
    transform: scale(0.96);
  }
  .test-btn.recording {
    color: var(--text-0);
    background: color-mix(in srgb, var(--rec) 24%, transparent);
  }
  .test-btn.recording:hover {
    background: color-mix(in srgb, var(--rec) 34%, transparent);
  }
  .test-btn .txt {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .rec-dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--rec);
    animation: rec-pulse 1.2s ease-in-out infinite;
  }
  .stop-sq {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: currentColor;
  }
  @keyframes rec-pulse {
    50% {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .rec-dot {
      animation: none;
    }
  }
</style>
