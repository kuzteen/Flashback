export type ExportStage =
  | 'ed.stage.prepare'
  | 'ed.stage.trim'
  | 'ed.stage.audio'
  | 'ed.stage.encode'
  | 'ed.stage.finish';

// Texto bajo la barra de exportación. Va atado al progreso real para que la barra y la frase
// avancen juntas; los tramos son orientativos, el backend no reporta fases.
const STAGES: [number, ExportStage][] = [
  [0.12, 'ed.stage.prepare'],
  [0.4, 'ed.stage.trim'],
  [0.7, 'ed.stage.audio'],
  [0.93, 'ed.stage.encode']
];

export function exportStage(progress: number): ExportStage {
  return STAGES.find(([until]) => progress < until)?.[1] ?? 'ed.stage.finish';
}
