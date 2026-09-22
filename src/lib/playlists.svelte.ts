import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import type { Clip } from './clips';
import { library } from './library.svelte';

type RawPlaylist = {
  id: string;
  name: string;
  created_ms: number;
  description: string;
  cover: string | null;
  cover_ms: number;
  clips: string[];
};

export type Playlist = {
  id: string;
  name: string;
  createdAt: Date;
  description: string;
  // La portada reemplazada conserva el nombre de archivo, así que la URL lleva la marca de
  // tiempo: sin ella el WebView seguiría sirviendo la anterior desde su caché.
  coverSrc: string | null;
  // Rutas, no ids: es la clave con la que el backend indexa el resto de metadatos del clip.
  clips: string[];
};

export const playlists = $state<{ list: Playlist[]; loaded: boolean }>({ list: [], loaded: false });

function toPlaylist(r: RawPlaylist): Playlist {
  return {
    id: r.id,
    name: r.name,
    createdAt: new Date(r.created_ms),
    description: r.description ?? '',
    coverSrc: r.cover ? `${convertFileSrc(r.cover)}?v=${r.cover_ms}` : null,
    clips: r.clips ?? []
  };
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
  const byPath = new Map(library.clips.map((c) => [c.path, c]));
  return p.clips.map((path) => byPath.get(path)).filter((c): c is Clip => !!c);
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
    const pl = toPlaylist(await invoke<RawPlaylist>('create_playlist', { name: clean }));
    playlists.list = [...playlists.list, pl];
    if (description.trim()) await updatePlaylist(pl.id, clean, description);
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
    const path = await invoke<string>('set_playlist_cover', { id, bytes: [...bytes] });
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
    playlists.list = playlists.list.map((p) =>
      p.id === id ? { ...p, clips: [...p.clips, ...paths.filter((x) => !p.clips.includes(x))] } : p
    );
  } catch (err) {
    console.error('playlist_add_clips', err);
  }
}

export async function removeFromPlaylist(id: string, paths: string[]) {
  if (paths.length === 0) return;
  try {
    await invoke('playlist_remove_clips', { id, paths });
    playlists.list = playlists.list.map((p) =>
      p.id === id ? { ...p, clips: p.clips.filter((c) => !paths.includes(c)) } : p
    );
  } catch (err) {
    console.error('playlist_remove_clips', err);
  }
}

export function inPlaylist(id: string, path: string): boolean {
  return findPlaylist(id)?.clips.includes(path) ?? false;
}

// Cuántas playlists contienen el clip: la tarjeta lo usa para marcarse sin recorrer todo.
export function playlistsWith(path: string): Playlist[] {
  return playlists.list.filter((p) => p.clips.includes(path));
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
