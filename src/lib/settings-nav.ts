export const SETTINGS_SECTIONS = [
  { slug: 'general', icon: 'settings-fill', labelKey: 'settings.nav.general' },
  { slug: 'capture', icon: 'diamond-fill', labelKey: 'settings.nav.capture' },
  { slug: 'shortcuts', icon: 'keyboard-fill', labelKey: 'settings.nav.shortcuts' },
  { slug: 'storage', icon: 'folder-open', labelKey: 'settings.nav.storage' }
] as const;

export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]['slug'];

// El botón de Ajustes de la barra lateral lleva a /settings, que reabre la última sección vista en
// vez de volver siempre a General.
let last: SettingsSection = 'general';

export function lastSettingsSection(): SettingsSection {
  return last;
}

export function rememberSettingsSection(pathname: string) {
  const slug = pathname.split('/')[2];
  const hit = SETTINGS_SECTIONS.find((s) => s.slug === slug);
  if (hit) last = hit.slug;
}
