import { invoke } from '@tauri-apps/api/core';

// Límite por minuto: un error dentro de un bucle de render no debe inundar el log.
const MAX_PER_MINUTE = 20;
let windowStart = 0;
let sent = 0;

function describe(v: unknown): string {
  if (v instanceof Error) return v.stack ? v.stack.split('\n').slice(0, 4).join(' | ') : `${v.name}: ${v.message}`;
  if (typeof v === 'string') return v;
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

function send(parts: unknown[]) {
  const now = Date.now();
  if (now - windowStart > 60_000) {
    windowStart = now;
    sent = 0;
  }
  if (++sent > MAX_PER_MINUTE) return;
  invoke('log_ui', { message: parts.map(describe).join(' ') }).catch(() => {});
}

// Los console.error de la interfaz ya marcan los fallos que se atrapan (un invoke que falla, por
// ejemplo), así que también van al log junto a los errores que nadie atrapó.
export function installUiLog() {
  const original = console.error.bind(console);
  console.error = (...args: unknown[]) => {
    original(...args);
    send(args);
  };
  window.addEventListener('error', (e) => send(['uncaught:', e.error ?? e.message]));
  window.addEventListener('unhandledrejection', (e) => send(['unhandled rejection:', e.reason]));
}
