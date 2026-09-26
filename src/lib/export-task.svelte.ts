import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { exportRequest, openExported, type ExportEdit, type ExportFormat } from './editor-state.svelte';
import { refreshLibrary } from './library.svelte';
import { CANCELLED } from './share.svelte';

export type ExportPhase = 'idle' | 'running' | 'done' | 'failed' | 'cancelled';

// La exportación vive fuera del editor: sigue y avisa aunque se cierre el editor o se cambie de
// clip, y el aviso se pinta a nivel de toda la app.
export const exportTask = $state({
  phase: 'idle' as ExportPhase,
  progress: 0,
  cancelling: false,
  dst: null as string | null,
  error: null as string | null
});

let job: { src: string; edit: ExportEdit; format: ExportFormat } | null = null;

export function startExport(format: ExportFormat): 'started' | 'busy' | 'empty' {
  if (exportTask.phase === 'running') return 'busy';
  const req = exportRequest();
  if (!req) return 'empty';
  job = { ...req, format };
  void run(job);
  return 'started';
}

export function retryExport() {
  if (job && exportTask.phase === 'failed') void run(job);
}

async function run(current: NonNullable<typeof job>) {
  Object.assign(exportTask, { phase: 'running', progress: 0, cancelling: false, dst: null, error: null });
  const unlisten = await listen<number>('export-progress', (e) => {
    if (exportTask.phase === 'running') exportTask.progress = e.payload;
  });
  try {
    const dst = await invoke<string>('edit_dest', { src: current.src, format: current.format });
    await invoke('export_clip', { src: current.src, dst, edit: current.edit });
    Object.assign(exportTask, { phase: 'done', progress: 1, dst });
    refreshLibrary().catch(() => {});
  } catch (e) {
    if (String(e).includes(CANCELLED)) {
      exportTask.phase = 'cancelled';
    } else {
      Object.assign(exportTask, { phase: 'failed', error: String(e) });
      console.error('export', e);
    }
  } finally {
    unlisten();
  }
}

export function cancelExport() {
  if (exportTask.phase !== 'running' || exportTask.cancelling) return;
  exportTask.cancelling = true;
  invoke('export_cancel').catch(() => {});
}

export function dismissExport() {
  if (exportTask.phase !== 'running') exportTask.phase = 'idle';
}

export async function viewExported() {
  const dst = exportTask.dst;
  dismissExport();
  if (dst) await openExported(dst);
}
