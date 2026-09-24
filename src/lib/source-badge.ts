import { isScreenSource } from './screen-source';

export type SourceBadge =
  | { kind: 'cover'; src: string }
  | { kind: 'game'; src: string }
  | { kind: 'initial'; letter: string }
  | { kind: 'screen' }
  | { kind: 'imported' };

// Qué imagen acompaña al origen de un clip. La portada puesta a mano manda siempre, aunque luego
// se cambie el juego; sin ella, el icono del juego elegido, y un nombre inventado sin icono
// lleva su inicial. Pantallas e importados conservan su icono propio.
export function sourceBadge(source: string, cover: string | null, icon: string | null): SourceBadge {
  if (cover) return { kind: 'cover', src: cover };
  const name = source.trim();
  if (!name) return { kind: 'imported' };
  if (isScreenSource(name)) return { kind: 'screen' };
  if (icon) return { kind: 'game', src: icon };
  return { kind: 'initial', letter: name[0].toUpperCase() };
}

// Juego a guardar al editar un clip: elegir otra vez el detectado (o dejarlo vacío) no es un
// cambio, y guardar null hace que el clip siga lo que detectó la captura.
export function gameOverride(chosen: string, detected: string): string | null {
  const name = chosen.trim();
  if (!name || name.toLowerCase() === detected.trim().toLowerCase()) return null;
  return name;
}
