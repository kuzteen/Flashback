import { convertFileSrc } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { openPath } from '@tauri-apps/plugin-opener';
import { captureFrame } from '$lib/editor-state.svelte';
import { t } from '$lib/i18n.svelte';
import { playback } from './playback.svelte';

// Estado de interfaz del editor abierto que comparten varias piezas. No es parte de la edición:
// no entra en el historial ni se guarda con el clip.
const FORMAT_KEY = 'flashback.editor.formatPanel';
const LOOK_KEY = 'flashback.editor.lookPanel';

function readOpen(key: string): boolean {
  try {
    return localStorage.getItem(key) === '1';
  } catch {
    return false;
  }
}

function saveOpen(key: string, open: boolean) {
  try {
    localStorage.setItem(key, open ? '1' : '0');
  } catch {}
}

class EditorUi {
  fs = $state(false);
  fsCtrlShow = $state(true);
  notice = $state<string | null>(null);
  shot = $state<string | null>(null);
  range = $state<{ from: number; to: number } | null>(null);
  guideAt = $state<number | null>(null);
  zoom = $state(1);
  // Alto del panel inferior elegido con el tirador; null = alto natural. Sobrevive al cambiar de
  // clip porque es una preferencia de quien edita, no del clip.
  dockH = $state<number | null>(null);
  toolsOpen = $state(false);
  blockMenu = $state<{ x: number; y: number; index: number } | null>(null);
  // Preferencia de quien edita: plegado por defecto y recordado entre sesiones.
  formatOpen = $state(readOpen(FORMAT_KEY));
  lookOpen = $state(readOpen(LOOK_KEY));
  // Antes/después de los ajustes de imagen: una línea parte el visor y `split` (0..1) es dónde.
  compare = $state(false);
  split = $state(0.5);

  toggleFormat() {
    this.formatOpen = !this.formatOpen;
    saveOpen(FORMAT_KEY, this.formatOpen);
  }

  toggleLook() {
    this.lookOpen = !this.lookOpen;
    saveOpen(LOOK_KEY, this.lookOpen);
  }

  private wasMaximized = false;
  private fsHideTimer: ReturnType<typeof setTimeout> | null = null;
  private noticeTimer: ReturnType<typeof setTimeout> | null = null;
  private shotTimer: ReturnType<typeof setTimeout> | null = null;

  reset() {
    for (const tm of [this.fsHideTimer, this.noticeTimer, this.shotTimer]) if (tm) clearTimeout(tm);
    this.fsHideTimer = this.noticeTimer = this.shotTimer = null;
    this.fs = false;
    this.fsCtrlShow = true;
    this.notice = null;
    this.shot = null;
    this.range = null;
    this.guideAt = null;
    this.zoom = 1;
    this.toolsOpen = false;
    this.blockMenu = null;
    this.compare = false;
    this.split = 0.5;
  }

  // Pantalla completa de la ventana nativa y no la API del navegador: en WebView2 con la ventana
  // maximizada, el fullscreen HTML deja el lienzo con el tamaño equivocado y el vídeo con franjas.
  // Tauri en maximizado adopta el área de trabajo (con barra de tareas), así que se desmaximiza
  // antes y se restaura al salir.
  async setFs(on: boolean) {
    this.fs = on;
    this.showFsCtrl();
    const win = getCurrentWindow();
    try {
      if (on) {
        this.wasMaximized = await win.isMaximized();
        if (this.wasMaximized) await win.unmaximize();
        await win.setFullscreen(true);
      } else {
        await win.setFullscreen(false);
        if (this.wasMaximized) await win.maximize();
      }
    } catch (e) {
      this.setNotice(`FS: ${e}`, 4000);
    }
  }

  // Los controles de pantalla completa se ocultan tras un rato sin actividad mientras suena, y
  // reaparecen al mover el ratón o pausar.
  showFsCtrl() {
    this.fsCtrlShow = true;
    if (this.fsHideTimer) clearTimeout(this.fsHideTimer);
    this.fsHideTimer = null;
    if (this.fs && playback.playing) {
      this.fsHideTimer = setTimeout(() => {
        if (this.fs && playback.playing) this.fsCtrlShow = false;
      }, 2200);
    }
  }

  hideFsCtrl() {
    if (this.fs) this.fsCtrlShow = false;
  }

  setNotice(msg: string, ms: number) {
    this.notice = msg;
    if (this.noticeTimer) clearTimeout(this.noticeTimer);
    this.noticeTimer = setTimeout(() => (this.notice = null), ms);
  }

  // Se copia al portapapeles leyendo el PNG ya guardado y no desde un canvas, que el protocolo
  // asset deja "sucio". Si el portapapeles falla, la captura sigue en disco.
  async screenshot() {
    try {
      const dst = await captureFrame(playback.shownMs());
      if (!dst) return;
      try {
        const blob = await (await fetch(convertFileSrc(dst))).blob();
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
      } catch (err) {
        console.error('clipboard', err);
      }
      this.shot = dst;
      if (this.shotTimer) clearTimeout(this.shotTimer);
      this.shotTimer = setTimeout(() => (this.shot = null), 6000);
    } catch (e) {
      this.setNotice(t('ed.shotError', { e: String(e) }), 5000);
    }
  }

  async openShot() {
    if (!this.shot) return;
    try {
      await openPath(this.shot);
    } catch (e) {
      console.error('openPath', e);
    }
  }
}

export const ui = new EditorUi();
