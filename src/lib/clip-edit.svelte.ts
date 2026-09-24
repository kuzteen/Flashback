import { invoke } from '@tauri-apps/api/core';
import type { Clip } from './clips';
import type { CoverChange } from './components/CoverFormDialog.svelte';
import { gameOverride } from './source-badge';
import { refreshLibrary, renameFavorite } from './library.svelte';
import { rekeyPlaylistClip } from './playlists.svelte';

// Rutas de los clips que se están editando: una desde el menú de la tarjeta, varias desde la
// selección de la biblioteca. Vacío = diálogo cerrado.
export const clipEdit = $state<{ paths: string[] }>({ paths: [] });

export function openClipEdit(paths: string[]) {
  clipEdit.paths = [...paths];
}

export function closeClipEdit() {
  clipEdit.paths = [];
}

export type ClipEditChange = {
  // Solo con un clip; null = sin cambios.
  name: string | null;
  // Juego elegido (el detectado deshace la edición); null = el campo no se tocó.
  game: string | null;
  cover: CoverChange;
};

export async function saveClipEdit(clips: Clip[], change: ClipEditChange) {
  let paths = clips.map((c) => c.path);
  // Primero el nombre: renombrar cambia la ruta, y juego y portada se guardan por ruta.
  if (clips.length === 1 && change.name !== null) {
    const clip = clips[0];
    const name = change.name.trim();
    if (name && name !== clip.title) {
      const newPath = await invoke<string>('rename_clip', { path: clip.path, newName: name });
      renameFavorite(clip.id, newPath.split(/[\\/]/).pop() ?? clip.id);
      rekeyPlaylistClip(clip.path, newPath);
      paths = [newPath];
    }
  }
  if (change.game !== null) {
    // Cada clip compara con lo que detectó él: en una selección mezclada, a unos les deshace la
    // edición y a otros se la pone.
    const groups = new Map<string | null, string[]>();
    clips.forEach((c, i) => {
      const game = gameOverride(change.game!, c.detected);
      groups.set(game, [...(groups.get(game) ?? []), paths[i]]);
    });
    for (const [game, group] of groups) await invoke('set_clip_game', { paths: group, game });
  }
  if (change.cover.bytes) {
    await invoke('set_clip_cover', change.cover.bytes, {
      headers: { 'x-clip-paths': encodeURIComponent(JSON.stringify(paths)) }
    });
  } else if (change.cover.clear) {
    await invoke('clear_clip_cover', { paths });
  }
  await refreshLibrary();
}
