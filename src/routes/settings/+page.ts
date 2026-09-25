import { redirect } from '@sveltejs/kit';
import { lastSettingsSection } from '$lib/settings-nav';

export function load() {
  redirect(307, `/settings/${lastSettingsSection()}`);
}
