<script lang="ts">
  import { pill } from '$lib/pill';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';
  import { afterNavigate, beforeNavigate } from '$app/navigation';
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

  const section = $derived(SETTINGS_SECTIONS.find((s) => page.url.pathname === `/settings/${s.slug}`));
  const subs = $derived(section?.subs ?? []);

  // Cada columna tiene su propio scroll: el menú no se desplaza con las opciones y cada sección
  // recuerda dónde se quedó (una sin visitar empieza arriba).
  let paneEl = $state<HTMLElement | null>(null);
  const scrollBySection = new Map<string, number>();
  beforeNavigate(({ from }) => {
    if (paneEl && from?.url) scrollBySection.set(from.url.pathname, paneEl.scrollTop);
  });
  afterNavigate(({ to }) => {
    if (paneEl && to?.url) paneEl.scrollTop = scrollBySection.get(to.url.pathname) ?? 0;
  });
  let activeSub = $state('');
  // Mientras se desplaza hasta una subsección pulsada, el scroll no cambia la elegida: al final de
  // la página una sección corta nunca llega arriba y el seguimiento marcaría la última.
  let locked = false;
  let unlockTimer = 0;
  // Un grupo cuenta como el que se está leyendo cuando su título pasa por aquí.
  const SPY_LINE = 64;

  $effect(() => {
    const list: readonly { id: string }[] = subs;
    const sc = paneEl;
    if (list.length < 2 || !sc) {
      activeSub = '';
      return;
    }
    const spy = () => {
      if (locked) return;
      const line = sc.getBoundingClientRect().top + SPY_LINE;
      let current: string = list[0].id;
      for (const sub of list) {
        const el = document.getElementById(sub.id);
        if (el && el.getBoundingClientRect().top <= line) current = sub.id;
      }
      if (sc.scrollTop > 0 && sc.scrollTop + sc.clientHeight >= sc.scrollHeight - 2) current = list[list.length - 1].id;
      activeSub = current;
    };
    const raf = requestAnimationFrame(spy);
    sc.addEventListener('scroll', spy, { passive: true });
    return () => {
      cancelAnimationFrame(raf);
      sc.removeEventListener('scroll', spy);
    };
  });

  // Al cambiar de sección, las subsecciones de la anterior se pliegan y las de la nueva se
  // despliegan a la vez, apagándose y encendiéndose mientras cambian de alto (como Discord).
  function collapse(node: HTMLElement) {
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    const s = slide(node, { duration: reduce ? 0 : 220, easing: cubicOut });
    return { ...s, css: (t: number, u: number) => `${s.css?.(t, u) ?? ''};opacity:${t}` };
  }

  function goSub(e: MouseEvent, id: string) {
    e.preventDefault();
    const el = document.getElementById(id);
    const sc = paneEl;
    if (!el || !sc) return;
    activeSub = id;
    locked = true;
    clearTimeout(unlockTimer);
    const unlock = () => {
      clearTimeout(unlockTimer);
      locked = false;
    };
    sc.addEventListener('scrollend', unlock, { once: true });
    unlockTimer = window.setTimeout(unlock, 1000);
    const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
    el.scrollIntoView({ behavior: reduce ? 'auto' : 'smooth', block: 'start' });
  }
</script>

<div class="settings">
  <header><h1>{t('settings.title')}</h1></header>

  <div class="body">
    <nav class="snav" aria-label={t('settings.title')}>
      {#each SETTINGS_SECTIONS as s (s.slug)}
        {@const href = `/settings/${s.slug}`}
        <a {href} class:active={page.url.pathname === href} aria-current={page.url.pathname === href ? 'page' : undefined}>
          <Icon name={s.icon} size={20} />
          <span>{t(s.labelKey)}</span>
        </a>
        {#if section?.slug === s.slug && s.subs.length > 1}
          <div class="subnav" transition:collapse use:pill={{ key: activeSub, axis: 'y', selector: '.on .tick' }}>
            {#each s.subs as sub (sub.id)}
              <a
                href="#{sub.id}"
                class:on={activeSub === sub.id}
                aria-current={activeSub === sub.id ? 'location' : undefined}
                onclick={(e) => goSub(e, sub.id)}
              >
                <span class="tick"></span>
                <span class="sub-label">{t(sub.labelKey)}</span>
              </a>
            {/each}
          </div>
        {/if}
      {/each}
      {#if version}<span class="ver label">Flashback {version}</span>{/if}
    </nav>

    <div class="pane-scroll" bind:this={paneEl}>
      <div class="pane">{@render children()}</div>
    </div>
  </div>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 22px 26px 0;
  }
  header {
    flex-shrink: 0;
    margin-bottom: 26px;
  }
  h1 {
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 224px minmax(0, 1fr);
    gap: 48px;
  }
  /* El hueco de la barra va a los dos lados para que el centro no se mueva entre una sección que
     desborda y otra que no. */
  .pane-scroll {
    min-height: 0;
    overflow-y: auto;
    scrollbar-gutter: stable both-edges;
  }
  /* Centrada respecto a la app, no al hueco que deja el menú: 616 = 960 / 2 + (224 + 48) / 2.
     Si la ventana no da para tanto, se queda pegada al menú. El margen derecho solo cuenta cuando
     el ancho no llega a 960 y separa los controles de la barra de scroll. */
  .pane {
    max-width: 960px;
    margin-left: max(0px, calc(50% - 616px));
    margin-right: 24px;
    padding-bottom: 48px;
  }

  .snav {
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: 24px;
  }
  .snav > a {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 44px;
    padding: 0 14px;
    font-size: 15px;
    font-weight: 500;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: background 0.15s ease, color 0.18s ease 0.06s;
  }
  .snav > a:hover {
    color: var(--text-1);
    background: var(--bg-hover);
  }
  .snav > a span {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  /* El fondo va en el propio enlace y no en un relleno que viaja: mientras la lista de encima se
     pliega, la sección elegida se mueve y el relleno se quedaría donde estaba. */
  .snav > a.active {
    color: var(--text-0);
    background: var(--bg-2);
  }

  /* La línea cae bajo el centro del icono de la sección (14 de margen + 20 / 2) y el texto, en la
     misma columna que el de la sección (14 + 20 + 12 de hueco = 46). */
  .subnav {
    position: relative;
    display: flex;
    flex-direction: column;
    margin: 4px 0 8px 23px;
  }
  /* La línea empieza y acaba donde lo haría el marcador en la primera y la última subsección. */
  .subnav::before {
    content: '';
    position: absolute;
    inset: 7px auto 7px 0;
    width: 2px;
    border-radius: 1px;
    background: var(--line-strong);
  }
  .subnav a {
    position: relative;
    display: flex;
    align-items: center;
    min-height: 36px;
    padding: 6px 12px 6px 23px;
    font-size: 14px;
    color: var(--text-2);
    transition: color 0.15s ease;
  }
  .subnav a:hover {
    color: var(--text-1);
  }
  .subnav a.on {
    color: var(--text-0);
  }
  .sub-label {
    line-height: 1;
    text-box: trim-both cap alphabetic;
  }
  .tick {
    position: absolute;
    inset: 7px auto 7px 0;
    width: 2px;
  }
  .subnav > :global(.slide-pill) {
    z-index: 1;
    border-radius: 1px;
    background: var(--text-0);
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
    padding: 0 14px;
    color: var(--text-3);
  }
</style>
