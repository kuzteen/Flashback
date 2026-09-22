import { commitSegments, editorState, redo, resetEdit, undo } from '$lib/editor-state.svelte';
import { cutAt, outToSeg, removeAt, removeRange, toggleDisabled } from '$lib/edit-model';
import type { ActionId } from '$lib/shortcuts';
import { playback } from './playback.svelte';
import { ui } from './ui.svelte';

// Desactivar el bloque que suena dejaría el cabezal dentro de un tramo ya ignorado: se pausa y se
// reengancha al tiempo válido más cercano.
export function toggleBlock(i: number) {
  if (!editorState.edit.segments[i]) return;
  playback.pause();
  commitSegments(toggleDisabled(editorState.edit.segments, i));
  playback.settle();
}

export function removeBlock(i: number) {
  if (commitSegments(removeAt(editorState.edit.segments, i))) playback.settle();
}

// Con un rango marcado, quitar actúa sobre el rango; si no, sobre el bloque seleccionado.
function removeSelection() {
  const r = ui.range;
  if (r) {
    if (commitSegments(removeRange(editorState.edit.segments, r.from, r.to))) {
      ui.range = null;
      playback.settle();
    }
    return;
  }
  removeBlock(editorState.active);
}

function cut() {
  const segs = editorState.edit.segments;
  const { index, srcMs } = outToSeg(segs, playback.outPos);
  if (commitSegments(cutAt(segs, index, srcMs))) editorState.active = index + 1;
}

export function runAction(id: ActionId) {
  switch (id) {
    case 'play':
      playback.toggle();
      break;
    case 'prevFrame':
      playback.stepFrame(-1);
      break;
    case 'nextFrame':
      playback.stepFrame(1);
      break;
    case 'cut':
      cut();
      break;
    case 'remove':
      removeSelection();
      break;
    case 'toggleDisable':
      toggleBlock(editorState.active);
      break;
    case 'undo':
      if (undo()) playback.settle();
      break;
    case 'redo':
      if (redo()) playback.settle();
      break;
    case 'fullscreen':
      void ui.setFs(!ui.fs);
      break;
    case 'reset':
      resetEdit();
      playback.settle();
      break;
  }
}
