// Número escrito a mano en el valor de un deslizador (clic derecho): se aceptan el % y la coma
// decimal, se redondea al paso del deslizador (1) y se ajusta a su rango. null si no es un número.
export function parseSliderValue(text: string, min: number, max: number): number | null {
  const clean = text.replace(/\s|%/g, '').replace(',', '.');
  if (!/^[+-]?(\d+\.?\d*|\.\d+)$/.test(clean)) return null;
  const n = Math.round(Number(clean));
  return Math.min(max, Math.max(min, n));
}

// Lo que admite el campo mientras se escribe: solo cifras y, si el deslizador baja de cero, un
// signo menos al principio.
export function digitsOnly(text: string, negative: boolean): string {
  const digits = text.replace(/\D/g, '');
  return negative && text.trimStart().startsWith('-') ? `-${digits}` : digits;
}
