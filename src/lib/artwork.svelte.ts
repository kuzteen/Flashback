import { convertFileSrc, invoke } from '@tauri-apps/api/core';

// El backend entrega el arte como ruta del archivo en caché (se carga por el protocolo asset);
// solo si no pudo escribirlo en disco llega como data URL.
export function artSrc(value: string | null | undefined): string | null {
  if (!value) return null;
  return value.startsWith('data:') ? value : convertFileSrc(value);
}

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
    .then((url) => (icons[name] = artSrc(url)))
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
    .then((url) => (heroes[name] = artSrc(url)))
    .catch(() => (heroes[name] = null));
}

// Iconos ligeros del buscador de "Editar clip" (solo Discord, 64 px), en su propia caché: los
// resultados que no se eligen no deben llenar la de iconos normales. Si el juego ya tiene su
// icono normal cargado, se usa ese.
const searchIcons = $state<Record<string, string | null>>({});
const searchAsked = new Set<string>();

export function searchIcon(name: string): string | null {
  return icons[name] ?? searchIcons[name] ?? null;
}

export function ensureSearchIcon(name: string) {
  if (name in icons || searchAsked.has(name)) return;
  searchAsked.add(name);
  invoke<string | null>('search_icon', { name })
    .then((url) => (searchIcons[name] = artSrc(url)))
    .catch(() => (searchIcons[name] = null));
}
