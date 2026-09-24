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

// Banners con la misma idea: el editor pinta el del juego del clip abierto en la barra superior,
// y abrir varios clips del mismo juego no debe repetir la petición.
const heroes = $state<Record<string, string | null>>({});
const heroAsked = new Set<string>();

export function gameHero(name: string): string | null {
  return heroes[name] ?? null;
}

export function ensureGameHero(name: string) {
  if (heroAsked.has(name)) return;
  heroAsked.add(name);
  invoke<string | null>('game_hero', { name, steamAppid: null })
    .then((url) => (heroes[name] = url ?? null))
    .catch(() => (heroes[name] = null));
}
