import { invoke } from '@tauri-apps/api/core';

// Caché de iconos compartida por toda la app. El backend ya los guarda en disco, pero sin esto
// cada tarjeta de playlist pediría por su cuenta los mismos juegos: aquí la petición se hace una
// sola vez por nombre y el resultado lo ven todos los componentes montados.
const icons = $state<Record<string, string | null>>({});
const asked = new Set<string>();

export function gameIcon(name: string): string | null {
  return icons[name] ?? null;
}

export function ensureGameIcon(name: string) {
  if (asked.has(name)) return;
  asked.add(name);
  invoke<string | null>('game_icon', { name, steamAppid: null })
    .then((url) => (icons[name] = url ?? null))
    .catch(() => (icons[name] = null));
}

export function initials(name: string): string {
  const parts = name
    .replace(/[^a-zA-Z0-9 ]/g, '')
    .split(/\s+/)
    .filter(Boolean);
  return parts.slice(0, 2).map((w) => w[0]).join('').toUpperCase() || '?';
}
