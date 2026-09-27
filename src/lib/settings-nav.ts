// subs: grupos de la página (id del SettingGroup) que el menú lista bajo la sección activa.
export const SETTINGS_SECTIONS = [
  {
    slug: 'general',
    icon: 'settings-fill',
    labelKey: 'settings.nav.general',
    subs: [
      { id: 'interface', labelKey: 'settings.group.interface' },
      { id: 'notifications', labelKey: 'settings.group.notifications' },
      { id: 'integrations', labelKey: 'settings.group.integrations' },
      { id: 'system', labelKey: 'settings.group.system' },
      { id: 'support', labelKey: 'settings.group.support' }
    ]
  },
  {
    slug: 'capture',
    icon: 'diamond-fill',
    labelKey: 'settings.nav.capture',
    subs: [
      { id: 'replay', labelKey: 'settings.group.replay' },
      { id: 'video', labelKey: 'settings.group.video' },
      { id: 'mic', labelKey: 'settings.group.mic' },
      { id: 'advanced', labelKey: 'settings.group.advanced' }
    ]
  },
  { slug: 'games', icon: 'gamepad', labelKey: 'settings.nav.games', subs: [] },
  { slug: 'shortcuts', icon: 'keyboard-fill', labelKey: 'settings.nav.shortcuts', subs: [] },
  {
    slug: 'storage',
    icon: 'folder-open',
    labelKey: 'settings.nav.storage',
    subs: [
      { id: 'clips', labelKey: 'settings.group.clips' },
      { id: 'cache', labelKey: 'settings.group.cache' }
    ]
  }
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
