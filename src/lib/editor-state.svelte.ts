import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Clip } from './clips';
import { EditHistory } from './edit-history';
import {
  fromSaved,
  fullClip,
  initialState,
  keptMs,
  toSaved,
  type EditState,
  type MixerState,
  type SavedEdit,
  type SavedSegment,
  type Segment,
} from './edit-model';

export type { MixerState } from './edit-model';

type ClipAudio = {
  system: string | null;
  mic: string | null;
  sys_peaks: number[] | null;
  mic_peaks: number[] | null;
  mix_peaks: number[] | null;
};

type EditorState = {
  clip: Clip | null;
  videoSrc: string | null;
  system: string | null;
  mic: string | null;
  loading: boolean;
  error: string | null;
  durationMs: number;
  frameTimes: number[];
  fps: number;
  sysPeaks: number[] | null;
  micPeaks: number[] | null;
  mixPeaks: number[] | null;
  exporting: boolean;
  exportProgress: number;
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
    system: null,
    mic: null,
    loading: false,
    error: null,
    durationMs: 0,
    frameTimes: [],
    fps: 30,
    sysPeaks: null,
    micPeaks: null,
    mixPeaks: null,
    exporting: false,
    exportProgress: 0,
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

const history = new EditHistory<EditState>();
// undefined mientras la edición guardada no ha llegado; null si no había ninguna.
let saved: SavedEdit | null | undefined;
let settled = false;
let gestureBefore: EditState | null = null;
let persistTimer: ReturnType<typeof setTimeout> | null = null;

// El historial guarda copias planas: un proxy de Svelte cambiaría bajo sus pies.
function snapshot(): EditState {
  return $state.snapshot(editorState.edit) as EditState;
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
  const before = snapshot();
  editorState.edit = next;
  history.record(before, snapshot());
  changed();
}

export function commitSegments(next: Segment[] | null): boolean {
  if (!next) return false;
  commit({ ...snapshot(), segments: next });
  return true;
}

// Un gesto continuo cambia el estado muchas veces y deja un solo paso de deshacer al soltar.
export function beginGesture() {
  gestureBefore = snapshot();
}

export function preview(next: EditState) {
  editorState.edit = next;
}

export function endGesture() {
  if (!gestureBefore) return;
  history.record(gestureBefore, snapshot());
  gestureBefore = null;
  changed();
}

export function undo(): boolean {
  const prev = history.undo(snapshot());
  if (!prev) return false;
  editorState.edit = prev;
  changed();
  return true;
}

export function redo(): boolean {
  const next = history.redo(snapshot());
  if (!next) return false;
  editorState.edit = next;
  changed();
  return true;
}

export function resetEdit() {
  if (editorState.durationMs <= 0) return;
  editorState.active = 0;
  commit({ ...snapshot(), segments: fullClip(editorState.durationMs) });
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
    const res = await invoke<ClipAudio>('prepare_clip_audio', { path });
    if (!same()) return;
    editorState.system = res.system ? convertFileSrc(res.system) : null;
    editorState.mic = res.mic ? convertFileSrc(res.mic) : null;
    editorState.sysPeaks = res.sys_peaks ?? null;
    editorState.micPeaks = res.mic_peaks ?? null;
    editorState.mixPeaks = res.mix_peaks ?? null;
  } catch (e) {
    if (same()) editorState.error = String(e);
  } finally {
    if (same()) editorState.loading = false;
  }
}

export async function persistEdit() {
  const path = editorState.clip?.path;
  if (!path || editorState.durationMs <= 0 || !settled) return;
  try {
    await invoke('save_clip_edit', { path, edit: toSaved(snapshot()) });
  } catch (e) {
    console.error('save_clip_edit', e);
  }
}

export async function exportClip(): Promise<string | undefined> {
  const clip = editorState.clip;
  if (!clip?.path) return;
  const s = toSaved(snapshot(), true);
  if (s.segments.length === 0) throw new Error('No hay bloques activos para exportar');
  editorState.exporting = true;
  editorState.exportProgress = 0;
  const unlisten = await listen<number>('export-progress', (e) => {
    editorState.exportProgress = e.payload;
  });
  try {
    const dst = await invoke<string>('edit_dest', { src: clip.path });
    await invoke('export_clip', { src: clip.path, dst, edit: { segments: s.segments, mixer: s.mixer } });
    return dst;
  } finally {
    unlisten();
    editorState.exporting = false;
    editorState.exportProgress = 0;
  }
}

// Se comparte el montaje, no el archivo: el backend materializa los cortes antes de arrastrar.
export function shareEdit(): { segments: SavedSegment[]; mixer: MixerState; keptSec: number } | null {
  const snap = snapshot();
  const s = toSaved(snap, true);
  if (s.segments.length === 0) return null;
  return { segments: s.segments, mixer: snap.mixer, keptSec: keptMs(snap.segments) / 1000 };
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
}
