// Otra acción que ya usa esa combinación: dos atajos iguales se pisan al registrarse y solo
// funcionaría uno.
export function takenBy<K extends Record<string, string>>(keys: K, action: keyof K, accel: string): keyof K | null {
  const want = accel.toLowerCase();
  for (const other of Object.keys(keys) as (keyof K)[]) {
    if (other !== action && keys[other].toLowerCase() === want) return other;
  }
  return null;
}
