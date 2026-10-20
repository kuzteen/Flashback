<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { fly, fade, slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { focusTrap } from '$lib/focus-trap';
  import { hoverPill } from '$lib/pill';
  import { flip } from '$lib/flip';
  import AudioSourceSettings from '$lib/components/settings/AudioSourceSettings.svelte';
  import ShortcutSettings from '$lib/components/settings/ShortcutSettings.svelte';
  import ShortcutKeys from '$lib/components/ShortcutKeys.svelte';
  import { goto } from '$app/navigation';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from '$lib/components/Icon.svelte';
  import Stepper from '$lib/components/Stepper.svelte';
  import SettingRow from '$lib/components/settings/SettingRow.svelte';
  import Switch from '$lib/components/settings/Switch.svelte';
  import {
    ONBOARDING_STEPS,
    cancelSkipConfirmation,
    completeOnboarding,
    confirmSkip,
    initialOnboardingFlow,
    nextOnboardingStep,
    openSkipConfirmation,
    previousOnboardingStep
  } from '$lib/onboarding-flow';
  import { persistCapturePrefs } from '$lib/capture-prefs';
  import {
    captureConfig,
    setFps,
    setMic,
    setMicDevice,
    setNoiseLevel,
    setNoiseSuppression,
    setQuality,
    setResolution,
    FPS_OPTIONS,
    QUALITY_OPTIONS,
    RES_OPTIONS,
    qualityLabel,
    resolutionLabel,
    type AudioMode,
    type QualityKey
  } from '$lib/capture-config.svelte';
  import { replay, setReplayEnabled, setReplaySeconds, BUFFER_OPTIONS } from '$lib/replay.svelte';
  import { hotkeys, SHORTCUT_ROWS } from '$lib/hotkeys.svelte';
  import { getLocale, setLocale, LOCALES, type Locale, t } from '$lib/i18n.svelte';

  type EncoderInfo = { available: string[]; auto: string };
  type AudioInput = { id: string; name: string };

  const progressSteps = Array.from({ length: ONBOARDING_STEPS });
  const stepKeys = [
    'onboarding.step.welcome',
    'onboarding.step.recording',
    'onboarding.step.audio',
    'onboarding.step.shortcuts',
    'onboarding.step.summary'
  ] as const;
  const MORPH_MS = 280;
  let shell = $state<HTMLElement | null>(null);
  let content = $state<HTMLElement | null>(null);
  let stepHeading = $state<HTMLElement | null>(null);
  let changing = $state(false);
  let bindingShortcut = $state(false);
  let direction = 1;
  let sizeAnimation: Animation | null = null;
  let alive = true;
  let micEpoch = 0;
  const reduceMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
  const localeOptions = LOCALES.map((locale) => ({ label: locale.label, value: locale.value }));
  const qualityOptions = $derived(QUALITY_OPTIONS.map((option) => ({ label: qualityLabel(option.key), value: option.key })));
  const resolutionOptions = RES_OPTIONS.map((option) => ({ label: option.label, value: option.height }));
  const fpsOptions = FPS_OPTIONS.map((fps) => ({ label: `${fps} FPS`, value: fps }));
  const replayOptions = BUFFER_OPTIONS.map((option) => ({ label: option.label, value: option.seconds }));
  const modeOptions = $derived([
    { label: t('audio.all'), value: 'all' as const },
    { label: t('audio.game.preset'), value: 'game' as const },
    { label: t('audio.apps'), value: 'apps' as const }
  ]);
  const shortcutRows = SHORTCUT_ROWS;

  function savedStep(): number {
    try {
      const value = Number(localStorage.getItem('flashback.onboarding.step'));
      return Number.isInteger(value) && value >= 0 && value < ONBOARDING_STEPS ? value : 0;
    } catch {
      return 0;
    }
  }

  let flow = $state(initialOnboardingFlow(savedStep()));
  let encoder = $state('Auto');
  let encoderInfo = $state<EncoderInfo>({ available: [], auto: '' });
  let encoderLoading = $state(true);
  let encoderEnumerationError = $state('');
  let encoderPreferenceError = $state('');
  let encoderTouched = $state(false);
  let encoderSaveError = $state('');
  let encoderWrite: Promise<void> = Promise.resolve();
  let audioInputs = $state<AudioInput[]>([]);
  let audioLoading = $state(true);
  let audioError = $state('');
  let appsSupported = $state(false);
  let appsSupportLoading = $state(true);
  let micTest = $state<'idle' | 'recording' | 'playing'>('idle');
  let micTestError = $state(false);
  let micBusy = $state(false);
  let micTimer = 0;
  let micMenuOpen = $state(false);
  let micMenuEl = $state<HTMLElement | null>(null);
  let pageError = $state('');
  let busy = $state(false);
  let skipBusy = $state(false);
  let skipError = $state(false);

  const encoderOptions = $derived.by(() => {
    const values = ['Auto', ...encoderInfo.available, 'Software'];
    if (encoder !== 'Auto' && !values.includes(encoder)) values.splice(values.length - 1, 0, encoder);
    return values.map((value) => ({
      label: value === 'Auto' && encoderInfo.auto ? `Auto (${encoderInfo.auto})` : value,
      value
    }));
  });
  const encoderError = $derived(encoderEnumerationError || encoderPreferenceError);
  const encoderCanFinish = $derived(
    !encoderLoading &&
      (!encoderEnumerationError || encoder !== 'Auto') &&
      (!encoderPreferenceError || encoderTouched)
  );
  const encoderUnavailable = $derived(
    encoder !== 'Auto' && encoder !== 'Software' && !encoderInfo.available.includes(encoder)
  );
  const recommended = $derived(
    captureConfig.quality === 'high' && captureConfig.resolution === 1080 && captureConfig.fps === 60
  );
  const selectedMicName = $derived(
    audioInputs.find((input) => input.id === captureConfig.micDevice)?.name ?? ''
  );
  const micUnavailable = $derived(Boolean(captureConfig.micDevice && audioInputs.length && !selectedMicName));
  onMount(async () => {
    encoderLoading = true;
    const [optionsResult, savedResult] = await Promise.allSettled([
      invoke<EncoderInfo>('encoder_options'),
      invoke<string>('get_encoder')
    ]);
    if (!alive) return;
    if (optionsResult.status === 'fulfilled') encoderInfo = optionsResult.value;
    else encoderEnumerationError = String(optionsResult.reason);
    if (savedResult.status === 'fulfilled') encoder = savedResult.value;
    else encoderPreferenceError = String(savedResult.reason);
    encoderLoading = false;

    const [inputsResult, supportedResult] = await Promise.allSettled([
        invoke<AudioInput[]>('list_audio_inputs'),
        invoke<boolean>('audio_apps_supported')
    ]);
    if (!alive) return;
    if (inputsResult.status === 'fulfilled') {
      audioInputs = inputsResult.value;
      if (!captureConfig.micDevice && inputsResult.value[0]) setMicDevice(inputsResult.value[0].id);
    } else audioError = String(inputsResult.reason);
    if (supportedResult.status === 'fulfilled') appsSupported = supportedResult.value;
    else audioError = audioError ? `${audioError}; ${String(supportedResult.reason)}` : String(supportedResult.reason);
    audioLoading = false;
    appsSupportLoading = false;
  });

  $effect(() => {
    try {
      localStorage.setItem('flashback.onboarding.step', String(flow.step));
    } catch {
      // La preferencia de progreso es auxiliar; la configuración principal ya se persiste en Rust.
    }
  });

  function stepIn(node: Element) {
    return fly(node, { x: direction * 12, duration: reduceMotion() ? 0 : 220, easing: cubicOut });
  }

  function stepOut(_node: Element) {
    const dir = direction;
    return {
      duration: reduceMotion() ? 0 : 140,
      easing: cubicOut,
      css: (t: number, u: number) => `position:absolute;inset:0 0 auto;opacity:${t};transform:translateX(${-dir * 8 * u}px)`
    };
  }

  function expand(node: Element) {
    return slide(node, { duration: reduceMotion() ? 0 : 220, easing: cubicOut });
  }

  function stopMicTest() {
    ++micEpoch;
    clearTimeout(micTimer);
    if (micTest !== 'idle') {
      invoke('mic_test_stop').catch(() => {});
      invoke('mic_test_discard').catch(() => {});
    }
    micTest = 'idle';
    micBusy = false;
  }

  async function move(delta: number) {
    if (busy || changing || bindingShortcut || flow.skipDialog) return;
    if (flow.step + delta < 0 || flow.step + delta >= ONBOARDING_STEPS) return;
    if (flow.step === 2) stopMicTest();
    changing = true;
    const from = shell?.getBoundingClientRect().height;
    if (shell && from) shell.style.height = `${from}px`;
    direction = delta;
    micMenuOpen = false;
    flow = delta > 0 ? nextOnboardingStep(flow) : previousOnboardingStep(flow);
    if (content) content.scrollTop = 0;
    await tick();
    const incoming = content?.querySelector<HTMLElement>(`[data-step="${flow.step}"]`);
    if (shell && content && incoming && from) {
      // El paso saliente sigue en el DOM durante su outro. Medir el panel completo aquí
      // conservaría su alto y provocaría un salto al terminar, especialmente al encoger.
      const box = getComputedStyle(shell);
      const padding = getComputedStyle(content);
      const chrome = from - content.getBoundingClientRect().height;
      const natural = chrome + incoming.offsetHeight + parseFloat(padding.paddingTop) + parseFloat(padding.paddingBottom);
      const to = Math.min(parseFloat(box.maxHeight), Math.max(parseFloat(box.minHeight), natural));
      shell.style.height = `${to}px`;
      if (!reduceMotion()) {
        sizeAnimation = shell.animate([{ height: `${from}px` }, { height: `${to}px` }], {
          duration: MORPH_MS, easing: 'cubic-bezier(0.5, 0, 0.2, 1)'
        });
        await sizeAnimation.finished.catch(() => {});
      }
    }
    if (shell) shell.style.height = 'auto';
    if (!alive) return;
    changing = false;
    if (content) content.scrollTop = 0;
    stepHeading?.focus({ preventScroll: true });
  }

  function goNext() {
    void move(1);
  }

  function goBack() {
    void move(-1);
  }

  async function chooseEncoder(value: string) {
    encoder = value;
    encoderTouched = true;
    encoderPreferenceError = '';
    encoderSaveError = '';
    try {
      const write = encoderWrite.catch(() => {}).then(() => invoke<void>('set_encoder', { enc: value }));
      encoderWrite = write;
      await write;
      if (encoder === value) encoderSaveError = '';
    } catch (error) {
      if (encoder === value) encoderSaveError = String(error);
    }
  }

  async function toggleMicTest() {
    if (micBusy) return;
    micBusy = true;
    const epoch = ++micEpoch;
    micTestError = false;
    try {
      if (micTest === 'playing') {
        await invoke('mic_test_stop').catch(() => {});
        clearTimeout(micTimer);
        micTest = 'idle';
        return;
      }
      if (micTest === 'recording') {
        clearTimeout(micTimer);
        const duration = await invoke<number>('mic_test_play', {
          level: captureConfig.noiseSuppression ? captureConfig.noiseLevel : 0
        });
        if (!alive || epoch !== micEpoch) {
          invoke('mic_test_stop').catch(() => {});
          invoke('mic_test_discard').catch(() => {});
          return;
        }
        if (duration > 0) {
          micTest = 'playing';
          micTimer = window.setTimeout(() => (micTest = 'idle'), duration + 150);
        } else {
          micTest = 'idle';
          micTestError = true;
        }
        return;
      }
      await invoke('mic_test_record', { device: captureConfig.micDevice });
      if (!alive || epoch !== micEpoch) {
        invoke('mic_test_stop').catch(() => {});
        invoke('mic_test_discard').catch(() => {});
        return;
      }
      micTest = 'recording';
      micTimer = window.setTimeout(() => void toggleMicTest(), 30_000);
    } catch {
      if (alive && epoch === micEpoch) {
        invoke('mic_test_stop').catch(() => {});
        micTest = 'idle';
        micTestError = true;
      }
    } finally {
      if (epoch === micEpoch) micBusy = false;
    }
  }

  async function finish() {
    if (busy || changing || bindingShortcut || !encoderCanFinish) return;
    stopMicTest();
    busy = true;
    pageError = '';
    try {
      await persistCapturePrefs();
      await encoderWrite;
      await invoke('set_encoder', { enc: encoder });
      await invoke('set_language', { lang: getLocale() });
      await invoke('finish_onboarding', { skipped: false });
      flow = completeOnboarding(flow);
      try {
        localStorage.removeItem('flashback.onboarding.step');
      } catch {
        // El marcador autoritativo se guarda en el archivo de ajustes de Rust.
      }
      await goto('/');
    } catch (error) {
      pageError = t('onboarding.saveError', { error: String(error) });
    } finally {
      busy = false;
    }
  }

  async function skipSetup() {
    if (skipBusy) return;
    stopMicTest();
    skipBusy = true;
    skipError = false;
    try {
      await persistCapturePrefs();

      await invoke('set_language', { lang: getLocale() });
      await invoke('finish_onboarding', { skipped: true });
      flow = confirmSkip(flow);
      try {
        localStorage.removeItem('flashback.onboarding.step');
      } catch {
        // La omisión queda persistida en Rust.
      }
      await goto('/');
    } catch {
      skipError = true;
    } finally {
      skipBusy = false;
    }
  }

  $effect(() => {
    if (!captureConfig.mic || flow.skipDialog) stopMicTest();
  });

  $effect(() => {
    if (!micMenuOpen) return;
    const close = (event: PointerEvent) => {
      if (!micMenuEl?.contains(event.target as Node)) micMenuOpen = false;
    };
    window.addEventListener('pointerdown', close);
    return () => window.removeEventListener('pointerdown', close);
  });

  onDestroy(() => {
    alive = false;
    sizeAnimation?.cancel();
    stopMicTest();
  });

  const replayLabel = $derived(replay.enabled ? BUFFER_OPTIONS.find((option) => option.seconds === replay.seconds)?.label ?? `${replay.seconds}s` : t('cap.off'));
  const audioModeValue = $derived(appsSupported ? captureConfig.audioMode : 'all');
  const audioModeLabel = $derived(modeOptions.find((option) => option.value === audioModeValue)?.label ?? t('audio.all'));
  const selectedEncoderLabel = $derived(encoder === 'Auto' && encoderInfo.auto ? `Auto (${encoderInfo.auto})` : encoder);
</script>

<svelte:window onkeydown={(event) => {
  if (event.key !== 'Escape') return;
  if (flow.skipDialog && !skipBusy) flow = cancelSkipConfirmation(flow);
  else if (micMenuOpen) {
    micMenuOpen = false;
    micMenuEl?.querySelector('button')?.focus();
  }
}} />

<main class="onboarding" aria-label={t('onboarding.title')} data-tauri-drag-region in:fade={{ duration: reduceMotion() ? 0 : 180 }}>
  <div class="shell" bind:this={shell} role="dialog" aria-modal="true" aria-label={t('onboarding.title')} tabindex="-1" inert={flow.skipDialog} use:focusTrap={{ initial: 'h1' }}>
    <ol class="progress" aria-label={t('onboarding.title')}>
      {#each progressSteps as _, i}
        <li class="progress-step" class:past={i < flow.step} class:current={i === flow.step} aria-current={i === flow.step ? 'step' : undefined}>
          {#if i < ONBOARDING_STEPS - 1}<span class="progress-line" class:filled={i < flow.step} aria-hidden="true"><span></span></span>{/if}
          <span class="progress-dot" aria-hidden="true">
            <span class="number" class:gone={i < flow.step}>{i + 1}</span>
            <span class="check" class:gone={i >= flow.step}><Icon name="check" size={13} sw={2.4} /></span>
          </span>
          <span class="progress-label">{t(stepKeys[i])}</span>
        </li>
      {/each}
    </ol>

    <div class="content" bind:this={content} inert={busy}>
      <div class="step-viewport">
      {#key flow.step}
        <div class="step-page" data-step={flow.step} in:stepIn out:stepOut>
          {#if flow.step > 0}
            <header class="step-heading">
              <h1 bind:this={stepHeading} tabindex="-1">{t(stepKeys[flow.step])}</h1>
            </header>
          {/if}
      {#if flow.step === 0}
        <section class="welcome pane">
          <div class="hero-mark"><img src="/flashback-mono.svg" alt="" /></div>
          <h1 bind:this={stepHeading} tabindex="-1">{t('onboarding.welcomeTitle')}</h1>
          <p class="intro">{t('onboarding.welcomeBody')}</p>
          <div class="language-pick">
            <span class="field-label">{t('onboarding.language')}</span>
            <Stepper value={getLocale()} options={localeOptions} onchange={(value) => setLocale(value as Locale)} ariaLabel={t('onboarding.language')} />
          </div>
        </section>
      {:else if flow.step === 1}
        <section class="pane recording-pane">
          <p class="intro">{t('onboarding.recordingIntro')}</p>
          <section class="setting-section">
            <div class="section-title"><span class="section-icon"><Icon name="bolt" size={24} /></span><div><h2>{t('onboarding.encoder')}</h2><p>{t('onboarding.encoderDesc')}</p></div>
              <div class="section-action">
                {#if encoderLoading}
                  <span class="state-text">{t('onboarding.encoderLoading')}</span>
                {:else}
                  <Stepper value={encoder} options={encoderOptions} onchange={chooseEncoder} ariaLabel={t('onboarding.encoder')} />
                {/if}
              </div>
            </div>
            {#if encoderError}<p class="message error"><Icon name="warning" size={15} />{t('onboarding.encoderError', { error: encoderError })}</p>{/if}
            {#if !encoderLoading && !encoderError && encoderInfo.available.length === 0}<p class="message">{t('onboarding.encoderNone')}</p>{/if}
            {#if encoderUnavailable}<p class="message warning">{t('settings.encoder.unavailable', { encoder })}</p>{/if}
            {#if encoderSaveError}<p class="message error">{encoderSaveError}</p>{/if}
          </section>

          <section class="setting-section">
            <div class="section-title"><span class="section-icon"><Icon name="clips-fill" size={24} /></span><div><h2>{t('settings.group.video')}</h2><p>{t('onboarding.qualityIntro')}</p></div></div>
            <SettingRow title={t('cap.quality')}><Stepper value={captureConfig.quality} options={qualityOptions} onchange={(value) => setQuality(value as QualityKey)} ariaLabel={t('cap.quality')} /></SettingRow>
            <SettingRow title={t('cap.resolution')}><Stepper value={captureConfig.resolution} options={resolutionOptions} onchange={(value) => setResolution(value as number)} ariaLabel={t('cap.resolution')} /></SettingRow>
            <SettingRow title={t('cap.fps')}><Stepper value={captureConfig.fps} options={fpsOptions} onchange={(value) => setFps(value as number)} ariaLabel={t('cap.fps')} /></SettingRow>
            {#if recommended}<p class="recommend"><Icon name="star" size={14} />{t('onboarding.recommended')}</p>{/if}
          </section>

          <section class="setting-section">
            <div class="section-title"><span class="section-icon"><Icon name="diamond-fill" size={24} /></span><div><h2>{t('settings.group.replay')}</h2><p>{t('onboarding.replayIntro')}</p></div>
              <div class="section-action"><Switch checked={replay.enabled} onchange={setReplayEnabled} label={t('settings.replayBg')} /></div>
            </div>
            <SettingRow title={t('settings.bufferLen')} sub disabled={!replay.enabled}><Stepper value={replay.seconds} options={replayOptions} onchange={(value) => setReplaySeconds(value as number)} ariaLabel={t('settings.bufferLen')} /></SettingRow>
          </section>
        </section>
      {:else if flow.step === 2}
        <section class="pane">
          <p class="intro">{t('onboarding.audioIntro')}</p>
          {#if audioError}<p class="message error"><Icon name="warning" size={15} />{t('onboarding.audioError', { error: audioError })}</p>{/if}
          <div class="setting-section"><AudioSourceSettings heading={false} /></div>

          <section class="setting-section">
            <div class="section-title"><span class="section-icon"><Icon name="mic" size={17} /></span><div><h2>{t('settings.group.mic')}</h2><p>{t('settings.mic.desc')}</p></div></div>
            <SettingRow title={t('settings.mic')}><Switch checked={captureConfig.mic} onchange={setMic} label={t('settings.mic')} /></SettingRow>
            <SettingRow title={t('settings.micInput')} sub disabled={!captureConfig.mic}>
              {#if audioLoading}
                <span class="state-text">{t('onboarding.audioLoading')}</span>
              {:else if audioInputs.length}
                <div class="dd" bind:this={micMenuEl}>
                  <button class="dd-trigger" aria-label={t('settings.micInput')} aria-haspopup="listbox" aria-expanded={micMenuOpen} onclick={() => (micMenuOpen = !micMenuOpen)}>
                    <span class="dd-value">{selectedMicName || t('onboarding.micUnavailable')}</span><Icon name="chevron-down" size={12} sw={2} />
                  </button>
                  {#if micMenuOpen}
                    <div class="dd-menu" role="listbox" aria-label={t('settings.micInput')} use:flip use:hoverPill={{ selector: '.dd-item', axis: 'y' }}>
                      {#each audioInputs as input (input.id)}
                        <button class="dd-item" role="option" aria-selected={captureConfig.micDevice === input.id} class:on={captureConfig.micDevice === input.id} onclick={() => { stopMicTest(); setMicDevice(input.id); micMenuOpen = false; }}>
                          <span>{input.name}</span><span class="dd-check"><Icon name="check" size={13} sw={2.2} /></span>
                        </button>
                      {/each}
                    </div>
                  {/if}
                </div>
              {:else}<span class="state-text">{t('onboarding.noMicrophones')}</span>{/if}
            </SettingRow>
            {#if micUnavailable}<p class="message warning">{t('onboarding.micUnavailable')}</p>{/if}
            {#if captureConfig.noiseSuppression}
              <div transition:expand>
              <SettingRow title={t('settings.noiseLevel')} sub disabled={!captureConfig.mic}>
                <div class="range-wrap"><input class="fader" type="range" min="0" max="100" value={captureConfig.noiseLevel} aria-label={t('settings.noiseLevel')} oninput={(event) => (captureConfig.noiseLevel = Number(event.currentTarget.value))} onchange={(event) => setNoiseLevel(Number(event.currentTarget.value))} /><span>{captureConfig.noiseLevel}%</span></div>
              </SettingRow>
              </div>
            {/if}
            <SettingRow title={t('settings.noise')} desc={t('settings.noise.desc')} disabled={!captureConfig.mic}><Switch checked={captureConfig.noiseSuppression} onchange={setNoiseSuppression} label={t('settings.noise')} /></SettingRow>
            <SettingRow title={t('settings.micTest')} sub disabled={!captureConfig.mic || !audioInputs.length}>
              <button class="outline-btn" onclick={toggleMicTest} disabled={micBusy || !captureConfig.mic || !audioInputs.length}>{micTest === 'recording' ? t('settings.micTest.listen') : micTest === 'playing' ? t('settings.micTest.stop') : t('settings.micTest.record')}</button>
            </SettingRow>
            {#if micTestError}<p class="message error">{t('onboarding.micTestError')}</p>{/if}
          </section>
        </section>
      {:else if flow.step === 3}
        <section class="pane">
          <p class="intro">{t('onboarding.shortcutIntro')}</p>
          <ShortcutSettings heading={false} oncapturechange={(active) => (bindingShortcut = active)} />
        </section>
      {:else}
        <section class="pane summary-pane">
          <p class="intro">{t('onboarding.summaryIntro')}</p>
          {#if encoderError}<p class="message error"><Icon name="warning" size={15} />{t('onboarding.encoderError', { error: encoderError })}</p>{/if}
          <div class="summary-card">
            {@render SummaryRow(t('onboarding.summary.encoder'), selectedEncoderLabel)}
            {@render SummaryRow(t('onboarding.summary.quality'), qualityLabel(captureConfig.quality))}
            {@render SummaryRow(t('onboarding.summary.resolution'), resolutionLabel(captureConfig.resolution))}
            {@render SummaryRow(t('onboarding.summary.fps'), `${captureConfig.fps} FPS`)}
            {@render SummaryRow(t('onboarding.summary.replay'), replay.enabled ? replayLabel : t('cap.off'))}
            {@render SummaryRow(t('onboarding.summary.audio'), audioModeLabel)}
            {@render SummaryRow(t('onboarding.summary.microphone'), captureConfig.mic ? (micUnavailable ? t('onboarding.micUnavailable') : selectedMicName || t('onboarding.noMicrophones')) : t('cap.off'))}
            {#each shortcutRows as row (row.key)}{@render SummaryRow(t(row.labelKey), hotkeys[row.key], true)}{/each}
          </div>
          {#if pageError}<p class="message error"><Icon name="warning" size={15} />{pageError}</p>{/if}
        </section>
      {/if}
        </div>
      {/key}
      </div>
    </div>

    <footer class="footer">
      <button class="skip" onclick={() => { skipError = false; flow = openSkipConfirmation(flow); }} disabled={busy || changing || bindingShortcut}>{t('onboarding.skip')}</button>
      <div class="footer-right">
        {#if flow.step > 0}<button class="back" onclick={goBack} disabled={busy || changing || bindingShortcut}>{t('onboarding.back')}</button>{/if}
        {#if flow.step < ONBOARDING_STEPS - 1}<button class="primary" disabled={busy || changing || bindingShortcut} onclick={goNext}><span>{t('onboarding.continue')}</span><Icon name="arrow-right" size={16} /></button>
        {:else}<button class="primary" onclick={finish} disabled={busy || changing || bindingShortcut || !encoderCanFinish}>{#if busy}<span class="spinner"></span>{/if}<span>{t('onboarding.start')}</span><Icon name="arrow-right" size={16} /></button>{/if}
      </div>
    </footer>
  </div>
</main>

{#snippet SummaryRow(label: string, value: string, keys = false)}
  <div class="summary-row"><span>{label}</span><strong>{#if keys}<ShortcutKeys tokens={value.split('+')} />{:else}{value}{/if}</strong></div>
{/snippet}

{#if flow.skipDialog}
  <div class="dialog-backdrop" role="presentation" transition:fade={{ duration: reduceMotion() ? 0 : 140 }}>
    <div class="skip-dialog" role="dialog" aria-modal="true" aria-labelledby="skip-title" aria-describedby="skip-message" tabindex="-1" use:focusTrap={{ initial: '.keep-setup', restore: true }} in:fly={{ y: 8, duration: reduceMotion() ? 0 : 160, easing: cubicOut }}>
  <div class="dialog-icon"><Icon name="diamond-fill" size={20} /></div>
      <h2 class="modal-title" id="skip-title">{t('onboarding.skip.title')}</h2>
      <p id="skip-message">{t('onboarding.skip.message')}</p>
      {#if skipError}<p class="message error">{t('onboarding.skipError')}</p>{/if}
      <div class="dialog-actions">
        <button class="back keep-setup" onclick={() => (flow = cancelSkipConfirmation(flow))} disabled={skipBusy}>{t('onboarding.skip.keep')}</button>
        <button class="danger" onclick={skipSetup} disabled={skipBusy}>{#if skipBusy}<span class="spinner"></span>{/if}{t('onboarding.skip.confirm')}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .onboarding {
    position: fixed; inset: 0; z-index: 100; display: grid; place-items: center;
    padding: calc(var(--topbar-h) + 24px) 24px 24px calc(var(--sidebar-w) + 24px);
    background: var(--scrim); backdrop-filter: blur(5px);
  }
  .shell {
    display: flex; flex-direction: column; width: min(780px, 100%);
    min-height: min(480px, calc(100dvh - var(--topbar-h) - 48px));
    max-height: calc(100dvh - var(--topbar-h) - 48px);
    padding: 28px 32px 20px; overflow: hidden;
    background: var(--surface); border: 1px solid var(--line-strong);
    border-radius: var(--r-lg); box-shadow: var(--shadow-dialog);
  }
  .progress {
    display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); flex-shrink: 0;
    list-style: none; padding: 0 0 24px; margin: 0 0 24px; border-bottom: 1px solid var(--line);
  }
  .progress-step { position: relative; display: flex; flex-direction: column; align-items: center; gap: 9px; }
  .progress-dot {
    position: relative; display: grid; place-items: center; width: 28px; height: 28px;
    color: var(--text-3); background: var(--bg-1); border: 1px solid var(--line-strong); border-radius: 50%;
    font-size: 11px; font-weight: 600; font-variant-numeric: tabular-nums;
    transition: color 180ms ease, background-color 180ms ease, border-color 180ms ease;
  }
  .current .progress-dot { color: var(--on-bright); background: var(--bright); border-color: var(--bright); }
  .past .progress-dot { color: var(--text-0); background: var(--bg-3); border-color: var(--line-strong); }
  .number, .check { grid-area: 1 / 1; display: grid; place-items: center; transition: opacity 140ms ease, transform 180ms ease; }
  .gone { opacity: 0; transform: translateY(3px); }
  .progress-label { color: var(--text-3); font-size: 11px; transition: color 180ms ease; }
  .current .progress-label { color: var(--text-0); }
  .past .progress-label { color: var(--text-2); }
  .progress-line {
    position: absolute; top: 13px; left: calc(50% + 21px); right: calc(-50% + 21px);
    height: 2px; overflow: hidden; background: var(--line); border-radius: 2px;
  }
  .progress-line > span { display: block; height: 100%; background: var(--text-2); transform: scaleX(0); transform-origin: left; transition: transform 220ms cubic-bezier(0.5, 0, 0.2, 1); }
  .progress-line.filled > span { transform: scaleX(1); }
  .content { flex: 1 1 auto; min-height: 0; overflow-y: auto; overflow-x: hidden; scrollbar-gutter: stable both-edges; margin: 0 -10px; padding: 0 10px 16px; }
  .step-viewport { position: relative; }
  .step-page { width: 100%; }
  .step-heading { margin-bottom: 8px; }
  h1 { font-size: 28px; font-weight: 600; letter-spacing: -0.025em; }
  h1:focus { outline: none; }
  .intro { margin-bottom: 24px; max-width: 560px; color: var(--text-2); font-size: 14px; line-height: 1.55; }
  .welcome { display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: 280px; padding: 12px 20px 24px; text-align: center; }
  .hero-mark { margin-bottom: 22px; }
  .hero-mark img { display: block; width: 66px; height: 66px; }
  .welcome h1 { font-size: 34px; }
  .welcome .intro { margin: 12px 0 28px; max-width: 440px; }
  .language-pick { display: flex; align-items: center; gap: 24px; }
  .field-label { font-size: 13px; color: var(--text-2); }
  .setting-section + .setting-section { margin-top: 24px; padding-top: 24px; border-top: 1px solid var(--line); }
  .section-title { display: flex; align-items: center; gap: 12px; margin-bottom: 8px; }
  .section-icon { display: grid; place-items: center; width: 28px; flex-shrink: 0; color: var(--text-1); }
  .section-title h2 { font-size: 16px; font-weight: 560; }
  .section-title p { margin-top: 4px; color: var(--text-2); font-size: 13px; line-height: 1.5; }
  .section-action { margin-left: auto; padding-left: 16px; flex-shrink: 0; }
  .recommend { display: flex; align-items: center; gap: 6px; color: var(--text-2); font-size: 12px; margin: 8px 0; }
  .message { display: flex; align-items: flex-start; gap: 8px; margin-top: 12px; color: var(--text-2); font-size: 12px; line-height: 1.5; overflow-wrap: anywhere; }
  .message :global(svg) { flex-shrink: 0; margin-top: 2px; }
  .message.error, .message.warning { color: var(--rec-text); }
  .state-text { max-width: 230px; font-size: 12px; color: var(--text-2); text-align: right; }
  .range-wrap { display: flex; align-items: center; gap: 10px; font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; }
  .range-wrap input { width: 120px; }
  .range-wrap > span { min-width: 34px; text-align: right; }
  .outline-btn {
    padding: 8px 12px; font-size: 12px; color: var(--text-1); background: var(--bg-0);
    border: 1px solid var(--line-strong); border-radius: var(--r-sm);
    transition: color 140ms ease, background 140ms ease;
  }
  .outline-btn:hover:not(:disabled) { color: var(--text-0); background: var(--bg-hover); }
  .summary-row { display: flex; align-items: center; justify-content: space-between; gap: 24px; min-height: 44px; border-bottom: 1px solid var(--line); font-size: 13px; }
  .summary-row:last-child { border: 0; }
  .summary-row > span { color: var(--text-2); }
  .summary-row strong { max-width: 65%; font-weight: 500; text-align: right; overflow-wrap: anywhere; }
  .footer { display: flex; align-items: center; justify-content: space-between; flex-shrink: 0; padding-top: 18px; margin-top: 4px; border-top: 1px solid var(--line); }
  .footer-right, .dialog-actions { display: flex; align-items: center; gap: 8px; }
  .skip, .back, .primary, .danger { display: inline-flex; align-items: center; justify-content: center; gap: 8px; height: 36px; padding: 0 14px; border-radius: var(--r-sm); font-size: 13px; font-weight: 500; transition: color 140ms ease, background 140ms ease; }
  .skip { color: var(--text-3); padding-inline: 8px; }
  .back { color: var(--text-1); }
  .skip:hover:not(:disabled), .back:hover:not(:disabled) { color: var(--text-0); background: var(--bg-hover); }
  .primary { min-width: 124px; background: var(--bright); color: var(--on-bright); }
  .primary:hover:not(:disabled) { background: #fff; }
  button:disabled { opacity: 0.4; cursor: default; }
  .spinner { width: 13px; height: 13px; border: 2px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: spin 700ms linear infinite; }
  .dialog-backdrop { position: fixed; inset: 0; z-index: 110; display: grid; place-items: center; padding: 24px; background: var(--scrim); }
  .skip-dialog { width: min(420px, 100%); padding: 24px; background: var(--bg-1); border: 1px solid var(--line-strong); border-radius: var(--r-md); box-shadow: var(--shadow-dialog); }
  .dialog-icon { display: none; }
  .skip-dialog p { margin-top: 12px; font-size: 13.5px; line-height: 1.5; color: var(--text-1); }
  .dialog-actions { justify-content: flex-end; margin-top: 22px; }
  .danger { color: var(--rec-text); background: color-mix(in srgb, var(--rec) 12%, transparent); }
  .danger:hover:not(:disabled) { background: color-mix(in srgb, var(--rec) 20%, transparent); }
  .dd { position: relative; }
  .dd-trigger { display: flex; align-items: center; gap: 12px; width: 240px; height: 34px; padding: 0 10px; border: 1px solid var(--line); border-radius: var(--r-sm); background: var(--bg-0); color: var(--text-1); font-size: 13px; }
  .dd-trigger:hover { border-color: var(--line-strong); }
  .dd-value { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: left; }
  .dd-menu { position: absolute; top: calc(100% + 6px); right: 0; z-index: 5; min-width: 100%; width: max-content; max-width: 360px; max-height: 200px; overflow-y: auto; padding: 4px; border: 1px solid var(--line-strong); border-radius: var(--r-md); background: var(--surface); box-shadow: var(--shadow-pop); }
  .dd-item { display: flex; align-items: center; justify-content: space-between; gap: 18px; width: 100%; min-height: 34px; padding: 7px 10px; border-radius: 4px; font-size: 12px; text-align: left; color: var(--text-2); }
  .dd-item.on { color: var(--text-0); }
  .dd-check { opacity: 0; }
  .dd-item.on .dd-check { opacity: 1; }
  .dd-menu > :global(.slide-pill) { border-radius: 4px; background: var(--bg-3); }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 760px) {
    .onboarding { padding: calc(var(--topbar-h) + 12px) 12px 12px calc(var(--sidebar-w) + 12px); }
    .shell { padding: 22px 20px 16px; max-height: calc(100dvh - var(--topbar-h) - 24px); }
    .progress { margin-bottom: 20px; padding-bottom: 20px; }
    .progress-label { font-size: 10px; }
    .section-title { flex-wrap: wrap; }
    .section-action { padding-left: 0; }
    .welcome { padding-inline: 0; }
    .welcome h1 { font-size: 28px; }
  }
  @media (prefers-reduced-motion: reduce) {
    .progress-dot, .number, .check, .progress-label, .progress-line > span { transition: none; }
    .spinner { animation: none; }
  }
</style>
