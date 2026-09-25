<script lang="ts">
  import Icon from '$lib/components/Icon.svelte';
  import SettingGroup from '$lib/components/settings/SettingGroup.svelte';
  import SettingRow from '$lib/components/settings/SettingRow.svelte';
  import { t } from '$lib/i18n.svelte';
  import {
    hotkeys,
    capture,
    setHotkey,
    labelFor,
    comboFromEvent,
    hasMainKey,
    eventHasUnsupportedKey,
    type HotkeyAction
  } from '$lib/hotkeys.svelte';

  const shortcutRows: { key: HotkeyAction; labelKey: string }[] = [
    { key: 'saveReplay', labelKey: 'settings.hk.saveReplay' },
    { key: 'record', labelKey: 'settings.hk.record' },
    { key: 'open', labelKey: 'settings.hk.open' }
  ];

  let rebinding = $state<HotkeyAction | null>(null);
  let liveTokens = $state<string[]>([]);
  let badKey = $state(false);
  let canSave = $derived(liveTokens.length > 0 && hasMainKey(liveTokens));

  function onKeyDown(e: KeyboardEvent) {
    if (!rebinding) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.code === 'Escape') {
      endCapture(false);
      return;
    }
    const combo = comboFromEvent(e);
    if (combo.length && hasMainKey(combo)) {
      liveTokens = combo;
      badKey = false;
    } else if (eventHasUnsupportedKey(e)) {
      badKey = true;
    }
  }

  function startCapture(action: HotkeyAction) {
    rebinding = action;
    liveTokens = [];
    badKey = false;
    // Soltar los atajos globales mientras se escucha, o el SO se traga la combinación.
    capture.active = true;
    window.addEventListener('keydown', onKeyDown, true);
  }

  function endCapture(save: boolean) {
    window.removeEventListener('keydown', onKeyDown, true);
    if (save && rebinding && liveTokens.length && hasMainKey(liveTokens)) {
      setHotkey(rebinding, liveTokens.join('+'));
    }
    rebinding = null;
    liveTokens = [];
    badKey = false;
    capture.active = false;
  }

  // Tocar el atajo inicia la captura; se guarda con el botón ✓ y ESC cancela. Tocar
  // otra fila cancela la reasignación en curso sin guardar.
  function startRebind(action: HotkeyAction) {
    if (rebinding === action) return;
    if (rebinding) endCapture(false);
    startCapture(action);
  }

  $effect(() => {
    return () => endCapture(false);
  });
</script>

<SettingGroup title={t('settings.group.global')}>
  {#each shortcutRows as row (row.key)}
    <SettingRow title={t(row.labelKey)}>
      <div class="hk-edit">
        <button
          type="button"
          class="combo mono"
          class:rec={rebinding === row.key}
          class:bad={rebinding === row.key && badKey && liveTokens.length === 0}
          onclick={() => startRebind(row.key)}
          aria-label={t('settings.hk.changeAria', { label: t(row.labelKey) })}
        >
          <span class="combo-text">
            {#if rebinding === row.key}
              {#if liveTokens.length}
                {labelFor(liveTokens.join('+'))}
              {:else if badKey}
                {t('settings.hk.badKey')}
              {:else}
                {t('settings.hk.pressKey')}
              {/if}
            {:else}
              {labelFor(hotkeys[row.key])}
            {/if}
          </span>
        </button>
        {#if rebinding === row.key}
          <button
            type="button"
            class="combo-save"
            disabled={!canSave}
            onclick={() => endCapture(true)}
            aria-label={t('settings.hk.saveAria')}
          >
            <Icon name="check" size={12} sw={3} />
          </button>
        {/if}
      </div>
    </SettingRow>
  {/each}
</SettingGroup>

<p class="hint">{@html t('settings.shortcuts.hint')}</p>

<style>
  .hint {
    margin: 12px 2px 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .hint :global(strong) {
    color: var(--text-1);
    font-weight: 600;
  }

  .hk-edit {
    position: relative;
    display: flex;
    align-items: center;
  }
  .combo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 132px;
    height: 34px;
    padding: 0 14px;
    font-size: 12px;
    letter-spacing: 0.04em;
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }
  /* Recortada a la altura de la mayúscula: sin descendentes, el hueco que la fuente les reserva la
     dejaba un poco alta en la caja. */
  .combo-text {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .combo:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .combo.rec {
    color: var(--accent);
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, var(--bg-0));
  }
  .combo.bad {
    color: var(--rec);
    border-color: var(--rec);
    background: color-mix(in srgb, var(--rec) 12%, var(--bg-0));
  }
  .combo-save {
    position: absolute;
    top: -8px;
    right: -8px;
    z-index: 5;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    color: var(--on-bright);
    background: var(--bright);
    border-radius: 999px;
    box-shadow: 0 3px 10px -2px rgba(0, 0, 0, 0.6);
    transition: transform 0.12s ease, opacity 0.12s ease;
  }
  .combo-save:hover:not(:disabled) {
    transform: scale(1.08);
  }
  .combo-save:active:not(:disabled) {
    transform: scale(0.94);
  }
  .combo-save:disabled {
    cursor: default;
  }
</style>
