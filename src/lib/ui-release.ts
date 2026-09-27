import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { flushEdit } from './editor-state.svelte';
import { exportTask } from './export-task.svelte';

let releasing = false;

async function hidden(): Promise<boolean> {
  try {
    return !(await getCurrentWindow().isVisible());
  } catch {
    return false;
  }
}

// Con la ventana en la bandeja la interfaz se descarga entera (Rust destruye el webview y libera
// su memoria). Antes se guarda lo pendiente y se espera a que acabe un export en curso, que avisa
// y refresca la biblioteca al terminar. Si la ventana vuelve a abrirse mientras tanto, no se toca.
export async function releaseWhenIdle() {
  if (releasing) return;
  releasing = true;
  try {
    await flushEdit();
    while (exportTask.phase === 'running') {
      await new Promise((r) => setTimeout(r, 500));
      if (!(await hidden())) return;
    }
    if (await hidden()) await invoke('release_ui');
  } catch (e) {
    console.error('release_ui', e);
  } finally {
    releasing = false;
  }
}
