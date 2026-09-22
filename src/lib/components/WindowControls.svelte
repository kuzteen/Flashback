<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Icon from './Icon.svelte';
  import { t } from '$lib/i18n.svelte';

  const appWindow = getCurrentWindow();
  let maximized = $state(false);

  // El tamaño mínimo lo impone tauri.conf.json (minWidth/minHeight); aquí solo seguimos el
  // estado de maximizado para alternar el icono del botón restaurar/maximizar.
  $effect(() => {
    appWindow.isMaximized().then((m) => (maximized = m));
    const unlisten = appWindow.onResized(async () => {
      maximized = await appWindow.isMaximized();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });
</script>

<div class="controls">
  <button class="ctl" aria-label={t('win.minimize')} onclick={() => appWindow.minimize()}>
    <Icon name="win-min" size={14} />
  </button>

  <button
    class="ctl"
    aria-label={maximized ? t('win.restore') : t('win.maximize')}
    onclick={() => appWindow.toggleMaximize()}
  >
    <Icon name="win-max" size={15} />
  </button>

  <button class="ctl close" aria-label={t('win.close')} onclick={() => appWindow.close()}>
    <Icon name="close-fill" size={12} />
  </button>
</div>

<style>
  .controls {
    display: flex;
    align-self: stretch;
  }
  .ctl {
    width: 46px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    transition: background 0.14s ease, color 0.14s ease;
  }
  .ctl:hover {
    background: var(--bg-2);
    color: var(--text-0);
  }
  .close:hover {
    background: var(--rec);
    color: #fff;
  }
</style>
