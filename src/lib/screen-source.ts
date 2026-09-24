// Las capturas de pantalla guardan el origen como "Pantalla N"; cualquier otro origen es un
// juego. Es la misma convención con la que el backend rellena el `source` al capturar.
export function isScreenSource(source: string): boolean {
  return /^(?:pantalla|screen)\b/i.test(source.trim());
}
