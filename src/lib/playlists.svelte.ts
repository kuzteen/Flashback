import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import type { Clip } from './clips';
import { library } from './library.svelte';

type RawClipRef = { path: string; added_ms: number; seen: boolean };

type RawPlaylist = {
  id: string;
  name: string;
  created_ms: number;
  description: string;
  cover: string | null;
  cover_ms: number;
  pos: number | null;
  clips: RawClipRef[];
};

// Rutas, no ids: es la clave con la que el backend indexa el resto de metadatos del clip.
// addedAt null son los clips que ya estaban antes de que se guardara la fecha.
export type PlaylistClip = { path: string; addedAt: Date | null; seen: boolean };

export type Playlist = {
  id: string;
  name: string;
  createdAt: Date;
  description: string;
  // La portada reemplazada conserva el nombre de archivo, así que la URL lleva la marca de
  // tiempo: sin ella el WebView seguiría sirviendo la anterior desde su caché.
  coverSrc: string | null;
  pos: number | null;
  clips: PlaylistClip[];
};

export const playlists = $state<{ list: Playlist[]; loaded: boolean }>({ list: [], loaded: false });

function toPlaylist(r: RawPlaylist): Playlist {
  return {
    id: r.id,
    name: r.name,
    createdAt: new Date(r.created_ms),
    description: r.description ?? '',
    coverSrc: r.cover ? `${convertFileSrc(r.cover)}?v=${r.cover_ms}` : null,
    pos: r.pos ?? null,
    clips: (r.clips ?? []).map((c) => ({
      path: c.path,
      addedAt: c.added_ms ? new Date(c.added_ms) : null,
      seen: !!c.seen
    }))
  };
}

// Marca de "recién añadido": se apaga al abrir el clip desde la playlist y, si no, sola a los
// siete días. Sin la caducidad, una playlist que se llenó y nunca se abrió arrastraría la
// marca en todos sus clips para siempre.
const FRESH_MS = 7 * 24 * 60 * 60 * 1000;

export function isFreshInPlaylist(c: PlaylistClip | undefined): boolean {
  if (!c || c.seen || !c.addedAt) return false;
  return Date.now() - c.addedAt.getTime() < FRESH_MS;
}

// Las que nunca se han movido van delante, de la más nueva a la más antigua: así se listaban
// antes de poder reordenar, y es donde tiene que aparecer una recién creada.
export function orderedPlaylists(list: Playlist[]): Playlist[] {
  return [...list].sort((a, b) => {
    if (a.pos === null && b.pos === null) return b.createdAt.getTime() - a.createdAt.getTime();
    if (a.pos === null) return -1;
    if (b.pos === null) return 1;
    return a.pos - b.pos;
  });
}

export async function reorderPlaylists(ids: string[]) {
  const pos = new Map(ids.map((id, i) => [id, i]));
  playlists.list = playlists.list.map((p) => ({ ...p, pos: pos.get(p.id) ?? null }));
  try {
    await invoke('reorder_playlists', { ids });
  } catch (err) {
    console.error('reorder_playlists', err);
    refreshPlaylists();
  }
}

export async function refreshPlaylists() {
  try {
    const raw = await invoke<RawPlaylist[]>('list_playlists');
    playlists.list = raw.map(toPlaylist);
  } catch {
    playlists.list = [];
  }
  playlists.loaded = true;
}

export function findPlaylist(id: string): Playlist | undefined {
  return playlists.list.find((p) => p.id === id);
}

// Las rutas guardadas pueden apuntar a clips que ya no están (borrados desde el Explorador);
// resolverlas contra la biblioteca las filtra y, de paso, conserva el orden de la playlist.
export function playlistClips(p: Playlist): Clip[] {
  const byPath = clipsByPath();
  return p.clips.map((c) => byPath.get(c.path)).filter((c): c is Clip => !!c);
}

// Un solo mapa para todas las tarjetas de playlist, rehecho solo cuando cambia la biblioteca:
// antes cada tarjeta indexaba la biblioteca entera por su cuenta en cada recarga. Vale cachear
// por identidad porque library.clips solo se reemplaza entero, nunca se muta en sitio.
let indexed: Clip[] | null = null;
let byPathCache = new Map<string, Clip>();

function clipsByPath(): Map<string, Clip> {
  const list = library.clips;
  if (list !== indexed) {
    byPathCache = new Map(list.map((c) => [c.path, c]));
    indexed = list;
  }
  return byPathCache;
}

// Espejo en memoria de playlists::rekey: al renombrar, el backend ya movió la ruta en el
// índice, y sin esto la playlist seguiría apuntando a la vieja y el clip desaparecería de ella
// hasta recargar.
export function rekeyPlaylistClip(oldPath: string, newPath: string) {
  const has = (p: Playlist) => p.clips.some((c) => c.path === oldPath);
  if (!playlists.list.some(has)) return;
  playlists.list = playlists.list.map((p) =>
    has(p)
      ? { ...p, clips: p.clips.map((c) => (c.path === oldPath ? { ...c, path: newPath } : c)) }
      : p
  );
}

// La portada llega como bytes y no como ruta: al crear todavía no hay id con el que nombrar
// el PNG, así que el diálogo la retiene y se sube en cuanto el backend devuelve la playlist.
export async function createPlaylist(
  name: string,
  description = '',
  cover: Uint8Array | null = null
): Promise<Playlist | null> {
  const clean = name.trim();
  if (!clean) return null;
  try {
    const pl = toPlaylist(
      await invoke<RawPlaylist>('create_playlist', { name: clean, description: description.trim() })
    );
    playlists.list = [...playlists.list, pl];
    if (cover) await setPlaylistCover(pl.id, cover);
    return findPlaylist(pl.id) ?? pl;
  } catch (err) {
    console.error('create_playlist', err);
    return null;
  }
}

export async function updatePlaylist(id: string, name: string, description: string) {
  const clean = name.trim();
  if (!clean) return;
  const desc = description.trim();
  try {
    await invoke('update_playlist', { id, name: clean, description: desc });
    playlists.list = playlists.list.map((p) =>
      p.id === id ? { ...p, name: clean, description: desc } : p
    );
  } catch (err) {
    console.error('update_playlist', err);
  }
}

export async function setPlaylistCover(id: string, bytes: Uint8Array) {
  try {
    const path = await invoke<string>('set_playlist_cover', bytes, {
      headers: { 'x-playlist-id': id }
    });
    const src = `${convertFileSrc(path)}?v=${Date.now()}`;
    playlists.list = playlists.list.map((p) => (p.id === id ? { ...p, coverSrc: src } : p));
  } catch (err) {
    console.error('set_playlist_cover', err);
  }
}

export async function clearPlaylistCover(id: string) {
  try {
    await invoke('clear_playlist_cover', { id });
    playlists.list = playlists.list.map((p) => (p.id === id ? { ...p, coverSrc: null } : p));
  } catch (err) {
    console.error('clear_playlist_cover', err);
  }
}

export async function deletePlaylist(id: string) {
  try {
    await invoke('delete_playlist', { id });
    playlists.list = playlists.list.filter((p) => p.id !== id);
  } catch (err) {
    console.error('delete_playlist', err);
  }
}

export async function addToPlaylist(id: string, paths: string[]) {
  if (paths.length === 0) return;
  try {
    await invoke('playlist_add_clips', { id, paths });
    const addedAt = new Date();
    playlists.list = playlists.list.map((p) =>
      p.id === id
        ? {
            ...p,
            clips: [
              ...p.clips,
              ...paths
                .filter((x) => !p.clips.some((c) => c.path === x))
                .map((path) => ({ path, addedAt, seen: false }))
            ]
          }
        : p
    );
  } catch (err) {
    console.error('playlist_add_clips', err);
  }
}

// Lo quitado se devuelve con su índice y su referencia completa: es lo que necesita
// restoreToPlaylist para deshacer sin que los clips vuelvan como recién añadidos al final.
export type RemovedClip = { index: number; clip: PlaylistClip };

export async function removeFromPlaylist(id: string, paths: string[]): Promise<RemovedClip[]> {
  if (paths.length === 0) return [];
  const removed: RemovedClip[] = [];
  findPlaylist(id)?.clips.forEach((clip, index) => {
    if (paths.includes(clip.path)) removed.push({ index, clip });
  });
  try {
    await invoke('playlist_remove_clips', { id, paths });
    playlists.list = playlists.list.map((p) =>
      p.id === id ? { ...p, clips: p.clips.filter((c) => !paths.includes(c.path)) } : p
    );
    return removed;
  } catch (err) {
    console.error('playlist_remove_clips', err);
    return [];
  }
}

export async function restoreToPlaylist(id: string, removed: RemovedClip[]) {
  if (removed.length === 0) return;
  const entries = removed.map(({ index, clip }) => [
    index,
    { path: clip.path, added_ms: clip.addedAt?.getTime() ?? 0, seen: clip.seen }
  ]);
  try {
    await invoke('playlist_restore_clips', { id, entries });
    playlists.list = playlists.list.map((p) => {
      if (p.id !== id) return p;
      const clips = [...p.clips];
      for (const { index, clip } of [...removed].sort((a, b) => a.index - b.index)) {
        if (clips.some((c) => c.path === clip.path)) continue;
        clips.splice(Math.min(index, clips.length), 0, clip);
      }
      return { ...p, clips };
    });
  } catch (err) {
    console.error('playlist_restore_clips', err);
    refreshPlaylists();
  }
}

// Se aplica en memoria antes de escribir para que la tarjeta caiga en su hueco al soltar, sin
// esperar al IPC. Si el backend falla se recarga el índice y vuelve el orden guardado.
export async function reorderPlaylist(id: string, paths: string[]) {
  const pl = findPlaylist(id);
  if (!pl) return;
  const byPath = new Map(pl.clips.map((c) => [c.path, c]));
  const clips = paths.map((p) => byPath.get(p)).filter((c): c is PlaylistClip => !!c);
  playlists.list = playlists.list.map((p) => (p.id === id ? { ...p, clips } : p));
  try {
    await invoke('playlist_set_clips', { id, paths });
  } catch (err) {
    console.error('playlist_set_clips', err);
    refreshPlaylists();
  }
}

// Abrir el clip desde la playlist es lo que lo da por visto. Se escribe en el índice y no solo
// en memoria: si no, la marca volvería entera al reiniciar la app.
export async function markPlaylistClipSeen(id: string, path: string) {
  const pl = findPlaylist(id);
  if (!pl?.clips.some((c) => c.path === path && !c.seen)) return;
  playlists.list = playlists.list.map((p) =>
    p.id === id
      ? { ...p, clips: p.clips.map((c) => (c.path === path ? { ...c, seen: true } : c)) }
      : p
  );
  try {
    await invoke('playlist_mark_seen', { id, path });
  } catch (err) {
    console.error('playlist_mark_seen', err);
  }
}

export function inPlaylist(id: string, path: string): boolean {
  return findPlaylist(id)?.clips.some((c) => c.path === path) ?? false;
}

// Cuántas playlists contienen el clip: la tarjeta lo usa para marcarse sin recorrer todo.
export function playlistsWith(path: string): Playlist[] {
  return playlists.list.filter((p) => p.clips.some((c) => c.path === path));
}

// Modal de edición: vive en el layout, así que solo se guarda aquí qué playlist está abierta.
// Crear usa el mismo modal sin id, porque la playlist no existe hasta que se confirma.
export const playlistEdit = $state<{ id: string | null; creating: boolean }>({
  id: null,
  creating: false
});

export function openPlaylistEdit(id: string) {
  playlistEdit.id = id;
  playlistEdit.creating = false;
}

export function openPlaylistCreate() {
  playlistEdit.id = null;
  playlistEdit.creating = true;
}

export function closePlaylistEdit() {
  playlistEdit.id = null;
  playlistEdit.creating = false;
}
