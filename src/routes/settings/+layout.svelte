<script lang="ts">
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';
  import { getVersion } from '@tauri-apps/api/app';
  import Icon from '$lib/components/Icon.svelte';
  import { t } from '$lib/i18n.svelte';
  import { SETTINGS_SECTIONS, rememberSettingsSection } from '$lib/settings-nav';

  let { children }: { children: Snippet } = $props();

  let version = $state('');
  getVersion()
    .then((v) => (version = v))
    .catch(() => {});

  $effect(() => rememberSettingsSection(page.url.pathname));
</script>

<div class="settings">
  <header><h1>{t('settings.title')}</h1></header>

  <div class="body">
    <nav class="snav" aria-label={t('settings.title')}>
      {#each SETTINGS_SECTIONS as s (s.slug)}
        {@const href = `/settings/${s.slug}`}
        <a {href} class:active={page.url.pathname === href} aria-current={page.url.pathname === href ? 'page' : undefined}>
          <Icon name={s.icon} size={17} />
          <span>{t(s.labelKey)}</span>
        </a>
      {/each}
      {#if version}<span class="ver label">Flashback {version}</span>{/if}
    </nav>

    <div class="pane">{@render children()}</div>
  </div>
</div>

<style>
  .settings {
    padding: 22px 26px 48px;
  }
  header {
    margin-bottom: 26px;
  }
  h1 {
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .body {
    display: grid;
    grid-template-columns: 196px minmax(0, 720px);
    gap: 36px;
    align-items: start;
  }

  .snav {
    position: sticky;
    top: 22px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .snav a {
    display: flex;
    align-items: center;
    gap: 11px;
    height: 38px;
    padding: 0 12px;
    font-size: 13.5px;
    font-weight: 500;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: background 0.15s ease, color 0.15s ease;
  }
  .snav a:hover {
    color: var(--text-1);
    background: var(--bg-hover);
  }
  .snav a span {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .snav a.active {
    color: var(--text-0);
    background: var(--bg-2);
  }
  /* Botón de las secciones (Abrir, Cambiar, Limpiar caché, Elegir…). La etiqueta va recortada a
     la altura de la mayúscula: sin descendentes, el hueco que la fuente les reserva la dejaba un
     poco alta respecto al icono. */
  .pane :global(.btn) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    flex-shrink: 0;
    height: 36px;
    padding: 0 15px;
    font-size: 13px;
    color: var(--text-1);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    transition: background 0.15s ease, color 0.15s ease, opacity 0.15s ease;
  }
  .pane :global(.btn:hover:not(:disabled)) {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .pane :global(.btn:disabled) {
    cursor: default;
  }
  .pane :global(.btn .txt) {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .ver {
    margin-top: 16px;
    padding: 0 12px;
    color: var(--text-3);
  }
</style>
