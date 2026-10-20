import { editorState } from '$lib/editor-state.svelte';
import { PlaybackController } from './playback-controller';

// Los campos visibles son reactivos; la coordinación de los medios se prueba por separado.
const status = $state({ playing: false, outPos: 0 });
export const playback = new PlaybackController(() => editorState, status);
