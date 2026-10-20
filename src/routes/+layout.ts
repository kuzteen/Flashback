import { initCapturePrefs } from '$lib/capture-prefs';
import { initLocale } from '$lib/i18n.svelte';
import { isTauri, invoke } from '@tauri-apps/api/core';
import { error, redirect } from '@sveltejs/kit';

// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
export const ssr = false;

const LEGACY_PREF_KEYS = [
  'flashback.capture.fps',
  'flashback.capture.quality',
  'flashback.capture.resolution',
  'flashback.capture.mic',
  'flashback.capture.micDevice',
  'flashback.replay.enabled',
  'flashback.replay.seconds',
  'flashback.hotkeys'
];

export const load = async ({ url }: { url: URL }) => {
  await initLocale();
  let needsOnboarding = false;
  if (isTauri()) {
    let legacyLocalData = false;
    try {
      legacyLocalData =
        LEGACY_PREF_KEYS.some((key) => localStorage.getItem(key) !== null) ||
        Object.keys(localStorage).some((key) => key.startsWith('flashback.'));
      needsOnboarding = await invoke<boolean>('begin_onboarding', { legacyLocalData });
    } catch (cause) {
      console.error('begin_onboarding', cause);
      throw error(500, 'Flashback could not load its onboarding state.');
    }
  }
  await initCapturePrefs();
  if (needsOnboarding && url.pathname !== '/onboarding') throw redirect(307, '/onboarding');
  if (!needsOnboarding && url.pathname === '/onboarding') throw redirect(307, '/');
};
