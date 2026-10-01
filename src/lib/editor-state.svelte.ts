import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { artSrc } from './artwork.svelte';
import { captureConfig } from './capture-config.svelte';
import type { Clip } from './clips';
import { EditHistory } from './edit-history';
import { clearFilmstrip, loadFilmstrip } from './filmstrip.svelte';
import { library, refreshLibrary } from './library.svelte';
import {
  DEFAULT_FORMAT,
  equalState,
  fromSaved,
  initialState,
  keptMs,
  toSaved,
  type EditState,
  type MixerState,
  type OutputFormat,
  type SavedEdit,
  type SavedSegment,
  type Segment,
} from './edit-model';

export type { MixerState } from './edit-model';

type ClipAudio = { tracks: { id: string; name: string; wav: string | null; peaks: number[] | null }[] };

// Pista de audio del editor. Sin `src` es la única del clip y suena el propio vídeo.
export type AudioLane = { id: string; name: string; src: string | null; peaks: number[] | null; icon: string | null };

type EditorState = {
  clip: Clip | null;
  videoSrc: string | null;
  tracks: AudioLane[];
  loading: boolean;
  error: string | null;
  durationMs: number;
  frameTimes: number[];
  fps: number;
  edit: EditState;
  // Bloque seleccionado: destino de quitar, desactivar y de los tiradores de recorte.
  active: number;
  canUndo: boolean;
  canRedo: boolean;
};

function blank(): EditorState {
  return {
    clip: null,
    videoSrc: null,
    tracks: [],
    loading: false,
    error: null,
    durationMs: 0,
    frameTimes: [],
    // 0 = aún no se sabe: la cabecera muestra un guion y el paso de fotograma usa 30.
    fps: 0,
    edit: initialState(0),
    active: 0,
    canUndo: false,
    canRedo: false,
  };
}

export const editorState = $state<EditorState>(blank());

// Clips en el orden de la rejilla desde la que se abrió el editor: cada página lo publica aquí
// para navegar al anterior / siguiente sin salir.
export const clipOrder = $state<{ list: Clip[] }>({ list: [] });

// Cada paso guarda también el bloque seleccionado: deshacer y rehacer devuelven la selección que
// había en ese punto, en vez de dejarla donde acabó o que un mismo índice señale otro bloque.
type Step = { edit: EditState; active: number };
const history = new EditHistory<Step>(100, (a, b) => equalState(a.edit, b.edit));
// undefined mientras la edición guardada no ha llegado; null si no había ninguna.
let saved: SavedEdit | null | undefined;
let settled = false;
let gestureBefore: Step | null = null;
let persistTimer: ReturnType<typeof setTimeout> | null = null;

// El historial guarda copias planas: un proxy de Svelte cambiaría bajo sus pies.
function snapshot(): EditState {
  return $state.snapshot(editorState.edit) as EditState;
}

function step(): Step {
  return { edit: snapshot(), active: editorState.active };
}

function syncHistory() {
  editorState.canUndo = history.canUndo;
  editorState.canRedo = history.canRedo;
}

function clampActive() {
  const n = editorState.edit.segments.length;
  editorState.active = Math.max(0, Math.min(editorState.active, n - 1));
}

function cancelPersist() {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = null;
}

function markEdited() {
  if (editorState.clip) editorState.clip.edited = true;
  cancelPersist();
  persistTimer = setTimeout(() => {
    persistTimer = null;
    void persistEdit();
  }, 400);
}

function changed() {
  clampActive();
  syncHistory();
  markEdited();
}

export function commit(next: EditState) {
  const before = step();
  editorState.edit = next;
  history.record(before, step());
  changed();
}

export function commitSegments(next: Segment[] | null): boolean {
  if (!next) return false;
  commit({ ...snapshot(), segments: next });
  return true;
}

// Un gesto continuo cambia el estado muchas veces y deja un solo paso de deshacer al soltar.
export function beginGesture() {
  gestureBefore = step();
}

export function preview(next: EditState) {
  editorState.edit = next;
}

export function endGesture() {
  if (!gestureBefore) return;
  history.record(gestureBefore, step());
  gestureBefore = null;
  changed();
}

function restore(to: Step) {
  editorState.edit = to.edit;
  editorState.active = to.active;
  changed();
}

export function undo(): boolean {
  const prev = history.undo(step());
  if (!prev) return false;
  restore(prev);
  return true;
}

export function redo(): boolean {
  const next = history.redo(step());
  if (!next) return false;
  restore(next);
  return true;
}

// Restablecer el montaje entero: cortes, mezcla, formato e imagen. Devolver el clip a intacto
// tiene que vaciar las cuatro partes, o el guardado se queda con la mezcla a medio ajustar y el
// clip sigue constando como editado.
export function resetEdit() {
  if (editorState.durationMs <= 0) return;
  commit(initialState(editorState.durationMs));
  editorState.active = 0;
}

// La duración la da el <video> y la edición guardada el backend; llegan en cualquier orden y el
// montaje solo se construye cuando están las dos.
function settle() {
  if (settled || editorState.durationMs <= 0 || saved === undefined) return;
  settled = true;
  editorState.edit = fromSaved(saved, editorState.durationMs);
  editorState.active = 0;
  history.clear();
  syncHistory();
}

export function setDuration(ms: number) {
  editorState.durationMs = ms;
  settle();
}

function resetAll() {
  cancelPersist();
  Object.assign(editorState, blank());
  history.clear();
  saved = undefined;
  settled = false;
  gestureBefore = null;
}

export function openEditor(clip: Clip) {
  resetAll();
  editorState.clip = clip;
  editorState.videoSrc = clip.previewSrc ?? null;
  editorState.loading = true;
  if (clip.path) loadFilmstrip(clip.path);
  else clearFilmstrip();
  void load(clip);
}

async function load(clip: Clip) {
  const path = clip.path;
  if (!path) {
    editorState.error = 'clip sin ruta';
    editorState.loading = false;
    return;
  }
  const same = () => editorState.clip?.path === path;
  const audio = invoke<ClipAudio>('prepare_clip_audio', { path });
  audio.catch(() => {});
  const [frameTimes, fps, edit] = await Promise.all([
    invoke<number[]>('frame_times', { path }).catch(() => [] as number[]),
    invoke<number>('clip_fps', { path }).catch(() => 0),
    invoke<SavedEdit>('load_clip_edit', { path }).catch(() => null),
  ]);
  if (!same()) return;
  editorState.frameTimes = frameTimes;
  if (fps > 0) editorState.fps = fps;
  saved = edit;
  settle();
  try {
    const res = await audio;
    if (!same()) return;
    editorState.tracks = res.tracks.map((t) => ({
      id: t.id,
      name: t.name,
      src: t.wav ? convertFileSrc(t.wav) : null,
      peaks: t.peaks ?? null,
      icon: null,
    }));
    void loadLaneIcons(path);
  } catch (e) {
    if (same()) editorState.error = String(e);
  } finally {
    if (same()) editorState.loading = false;
  }
}

// Icono de cada app, guardado por el backend desde que se grabó con ella (la ruta de la lista de
// Ajustes solo sirve para sacarlo la primera vez), y el del juego, el mismo de Ajustes > Juegos.
async function loadLaneIcons(path: string) {
  const same = () => editorState.clip?.path === path;
  const setIcon = (id: string, icon: string | null) => {
    const lane = editorState.tracks.find((l) => l.id === id);
    if (lane && icon) lane.icon = icon;
  };
  const apps = editorState.tracks.flatMap((l) => {
    if (!l.id.startsWith('app:')) return [];
    const exe = l.id.slice(4);
    const path = captureConfig.audioApps.find((a) => a.exe.toLowerCase() === exe)?.path || null;
    return [{ id: l.id, exe, path }];
  });
  const game = editorState.tracks.find((l) => l.id === 'game' && l.name && l.name !== 'Game');
  const [icons, gameUrl] = await Promise.all([
    Promise.all(apps.map((a) => invoke<string | null>('audio_app_icon', { exe: a.exe, path: a.path }).catch(() => null))),
    game ? invoke<string | null>('game_icon', { name: game.name, steamAppid: null }).catch(() => null) : null,
  ]);
  if (!same()) return;
  apps.forEach((a, i) => setIcon(a.id, icons[i] ?? null));
  if (game) setIcon(game.id, artSrc(gameUrl));
}

// Antes de descargar la interfaz: el guardado diferido del último cambio se perdería con ella.
export async function flushEdit() {
  if (!persistTimer) return;
  cancelPersist();
  await persistEdit();
}

// El backend responde si el montaje quedó guardado o se borró por no editar nada: el badge de la
// biblioteca se apaga con esa respuesta, en vez de quedarse encendido para el resto de la sesión.
export async function persistEdit() {
  const path = editorState.clip?.path;
  if (!path || editorState.durationMs <= 0 || !settled) return;
  try {
    const kept = await invoke<boolean>('save_clip_edit', { path, edit: toSaved(snapshot()) });
    if (editorState.clip?.path === path) editorState.clip.edited = kept;
  } catch (e) {
    console.error('save_clip_edit', e);
  }
}

export type ExportFormat = 'mp4' | 'mov';

export type ExportEdit = { segments: SavedSegment[]; mixer: SavedEdit['mixer']; format: OutputFormat; look: SavedEdit['look'] };

// El montaje se congela al pulsar Exportar: seguir editando o cambiar de clip mientras exporta
// no altera lo que se está escribiendo.
export function exportRequest(): { src: string; edit: ExportEdit } | null {
  const src = editorState.clip?.path;
  if (!src) return null;
  const s = toSaved(snapshot(), true);
  if (s.segments.length === 0) return null;
  return { src, edit: { segments: s.segments, mixer: s.mixer, format: s.format ?? DEFAULT_FORMAT, look: s.look } };
}

// Se comparte el montaje, no el archivo: el backend materializa los cortes antes de arrastrar.
export function shareEdit(): {
  segments: SavedSegment[];
  mixer: MixerState;
  format: OutputFormat;
  keptSec: number;
} | null {
  const snap = snapshot();
  const s = toSaved(snap, true);
  if (s.segments.length === 0) return null;
  return { segments: s.segments, mixer: snap.mixer, format: snap.format, keptSec: keptMs(snap.segments) / 1000 };
}

export async function captureFrame(timeMs: number): Promise<string | undefined> {
  if (!editorState.clip?.path) return;
  return await invoke<string>('capture_frame', { path: editorState.clip.path, timeMs });
}

// Se guarda antes de cambiar, cancelando el guardado diferido para que no se dispare ya con el
// clip nuevo cargado.
export async function navigateClip(dir: 1 | -1): Promise<void> {
  const id = editorState.clip?.id;
  if (!id) return;
  const i = clipOrder.list.findIndex((c) => c.id === id);
  const next = i >= 0 ? clipOrder.list[i + dir] : undefined;
  if (!next) return;
  cancelPersist();
  await persistEdit();
  openEditor(next);
}

export function closeEditor() {
  resetAll();
  clearFilmstrip();
}

// Abre en el editor un clip recién exportado. Si el editor está abierto se guarda antes su
// montaje, igual que al cambiar de clip, y se relee la biblioteca porque el exportado aún no
// estaba en ella.
export async function openExported(dst: string): Promise<boolean> {
  await refreshLibrary();
  const exported = library.clips.find((c) => c.path?.toLowerCase() === dst.toLowerCase());
  if (!exported) return false;
  cancelPersist();
  await persistEdit();
  openEditor(exported);
  return true;
}
