<script lang="ts">
  import SettingGroup from '$lib/components/settings/SettingGroup.svelte';
  import SettingRow from '$lib/components/settings/SettingRow.svelte';
  import { t } from '$lib/i18n.svelte';
  import { takenBy } from '$lib/hotkey-conflict';
  import {
    hotkeys,
    capture,
    hotkeyFailed,
    setHotkey,
    labelTokens,
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
  // Teclas que se están manteniendo ahora mismo, con la misma regla que la combinación que se
  // guarda (un modificador + una tecla): se pintan hundidas mientras siguen pulsadas.
  let held = $state<string[]>([]);
  let mainDown: string | null = null;
  const taken = $derived(rebinding && liveTokens.length ? takenBy(hotkeys, rebinding, liveTokens.join('+')) : null);
  let canSave = $derived(liveTokens.length > 0 && hasMainKey(liveTokens) && !taken);
  const shown = $derived(hasMainKey(held) || !liveTokens.length ? held : liveTokens);

  function labelOf(action: HotkeyAction) {
    return t(shortcutRows.find((r) => r.key === action)?.labelKey ?? '');
  }

  function refreshHeld(e: KeyboardEvent) {
    const mods: string[] = [];
    if (e.ctrlKey) mods.push('Control');
    if (e.altKey) mods.push('Alt');
    if (e.shiftKey) mods.push('Shift');
    if (e.metaKey) mods.push('Super');
    held = mainDown ? [...mods.slice(0, 1), mainDown] : mods.slice(0, 2);
  }

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
      mainDown = combo[combo.length - 1];
      badKey = false;
    } else if (eventHasUnsupportedKey(e)) {
      badKey = true;
    }
    refreshHeld(e);
  }

  function onKeyUp(e: KeyboardEvent) {
    if (!rebinding) return;
    e.preventDefault();
    e.stopPropagation();
    const combo = comboFromEvent(e);
    if (hasMainKey(combo) && combo[combo.length - 1] === mainDown) mainDown = null;
    refreshHeld(e);
    // Se guarda al soltar la combinación entera. Si choca con otra acción sigue escuchando, con el
    // aviso a la vista, para probar otra.
    if (held.length === 0 && canSave) endCapture(true);
  }

  function startCapture(action: HotkeyAction) {
    rebinding = action;
    liveTokens = [];
    held = [];
    mainDown = null;
    badKey = false;
    // Soltar los atajos globales mientras se escucha, o el SO se traga la combinación.
    capture.active = true;
    window.addEventListener('keydown', onKeyDown, true);
    window.addEventListener('keyup', onKeyUp, true);
  }

  function endCapture(save: boolean) {
    window.removeEventListener('keydown', onKeyDown, true);
    window.removeEventListener('keyup', onKeyUp, true);
    if (save && rebinding && canSave) {
      setHotkey(rebinding, liveTokens.join('+'));
    }
    rebinding = null;
    liveTokens = [];
    held = [];
    mainDown = null;
    badKey = false;
    capture.active = false;
  }

  // Tocar el atajo inicia la captura; se guarda al soltar las teclas y ESC cancela. Tocar
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
    {@const active = rebinding === row.key}
    <SettingRow title={t(row.labelKey)}>
      {#snippet info()}
        {#if active && taken}
          <p class="warn">{t('settings.hk.takenBy', { name: labelOf(taken) })}</p>
        {:else if !active && hotkeyFailed[row.key]}
          <p class="warn">{t('settings.hk.inUse')}</p>
        {/if}
      {/snippet}
      <div class="hk-edit">
        <button
          type="button"
          class="combo"
          class:rec={active}
          class:bad={active && ((badKey && shown.length === 0) || taken !== null)}
          class:failed={!active && hotkeyFailed[row.key]}
          onclick={() => startRebind(row.key)}
          aria-label={t('settings.hk.changeAria', { label: t(row.labelKey) })}
        >
          {#if active && shown.length === 0}
            <span class="combo-text">{badKey ? t('settings.hk.badKey') : t('settings.hk.pressKey')}</span>
          {:else}
            {@const tokens = active ? shown : hotkeys[row.key].split('+')}
            <span class="caps">
              {#each tokens as tok, i (tok)}
                {#if i > 0}<span class="plus">+</span>{/if}
                <kbd class="cap" class:down={active && held.includes(tok)}>{labelTokens(tok)[0]}</kbd>
              {/each}
            </span>
          {/if}
        </button>
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
    padding: 0 8px;
    font-size: 12px;
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
  }
  /* Recortada a la altura de la mayúscula: sin descendentes, el hueco que la fuente les reserva la
     dejaba un poco alta en la caja. */
  .combo-text {
    padding: 0 6px;
    letter-spacing: 0.04em;
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .caps {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .plus {
    font-size: 11px;
    color: var(--text-3);
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  /* Tecla física: el borde inferior grueso es su altura; hundida, baja lo que medía ese borde. */
  .cap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    font-family: inherit;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.03em;
    line-height: 1;
    color: var(--text-0);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-bottom-width: 3px;
    border-radius: 5px;
    transition: height 0.07s ease, border-bottom-width 0.07s ease, margin-top 0.07s ease;
  }
  .cap.down {
    height: 20px;
    border-bottom-width: 1px;
    margin-top: 2px;
    background: var(--bg-3);
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
  .combo.failed {
    border-color: color-mix(in srgb, var(--rec) 55%, var(--line));
  }
  .warn {
    margin-top: 3px;
    font-size: 12px;
    color: var(--rec-text);
  }
</style>
