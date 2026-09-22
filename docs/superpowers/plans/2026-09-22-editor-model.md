# Editor: modelo, historial y atajos — Plan de implementación

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Crear la lógica pura del editor nuevo (operaciones sobre bloques, imanes, quitar rango, historial de deshacer y tabla de atajos) con tests, sin tocar todavía nada visible.

**Architecture:** Tres módulos TypeScript sin Svelte ni IPC en `src/lib/`: `edit-model.ts` (estado editable y operaciones inmutables: cada función recibe segmentos y devuelve una lista nueva o `null` si la operación no procede), `edit-history.ts` (pilas de copias completas) y `shortcuts.ts` (tabla única de atajos + emparejado de teclas). El editor actual (`Editor.svelte`, `editor.svelte.ts`) no se toca; el plan 2 los sustituirá usando estos módulos.

**Tech Stack:** TypeScript 5.6, Vitest 3 (solo desarrollo), pnpm.

**Spec:** `docs/superpowers/specs/2026-09-22-editor-redesign-design.md` (secciones 2, 5 y 6).

## Global Constraints

- Sin dependencias de runtime nuevas. Vitest solo en `devDependencies`.
- Comentarios en español, solo para el *porqué* no obvio; nada de comentarios decorativos.
- Commits en inglés, autor único `kuzteen`; **ninguna** línea `Co-Authored-By` ni `Generated with`.
- Tiempos siempre en milisegundos (`number`). Posición en la timeline = `posMs`; tiempo de origen = `startMs`/`endMs`.
- `MIN_SEG_MS = 50`: ningún bloque puede quedar más corto.
- `crop_x` por defecto **0,5**, rango [0, 1]. Formato por defecto `{ kind: 'horizontal' }`.
- Historial: límite **100** pasos.
- Umbral de imán: lo convierte la UI de 8 px a ms; aquí se recibe ya en ms (`snapMs`), y `snapMs <= 0` desactiva el imán.
- Formato guardado compatible hacia atrás: `crop_x` y `format` opcionales al leer.

---

## Estructura de archivos

| Archivo | Responsabilidad |
|---|---|
| `vitest.config.ts` (nuevo) | Configuración mínima de Vitest para tests de TS puro (sin el plugin de SvelteKit). |
| `package.json` (modificar) | `vitest` en devDependencies y script `test`. |
| `src/lib/edit-model.ts` (nuevo) | Tipos del estado editable, carga/guardado compatible, mapeo de tiempos y operaciones. |
| `src/lib/edit-model.test.ts` (nuevo) | Tests del modelo. |
| `src/lib/edit-history.ts` (nuevo) | Historial de deshacer / rehacer por copias. |
| `src/lib/edit-history.test.ts` (nuevo) | Tests del historial. |
| `src/lib/shortcuts.ts` (nuevo) | Tabla de atajos, emparejado de eventos y tokens para mostrar. |
| `src/lib/shortcuts.test.ts` (nuevo) | Tests de atajos. |

---

### Task 1: Vitest y núcleo del modelo (tipos, carga/guardado, tiempos)

**Files:**
- Create: `vitest.config.ts`
- Modify: `package.json` (scripts y devDependencies)
- Create: `src/lib/edit-model.ts`
- Test: `src/lib/edit-model.test.ts`

**Interfaces:**
- Produces (usado por todas las tareas siguientes y por el plan 2):
  - `MIN_SEG_MS: 50`
  - `type Segment = { startMs: number; endMs: number; posMs: number; boundStartMs: number; boundEndMs: number; disabled: boolean; cropX: number }`
  - `type MixerState = { sys_vol: number; sys_muted: boolean; mic_vol: number; mic_muted: boolean }`
  - `DEFAULT_MIXER: MixerState`
  - `type OutputFormat = { kind: 'horizontal' } | { kind: 'vertical'; fill: 'crop' | 'fit' }`
  - `DEFAULT_FORMAT: OutputFormat`
  - `type EditState = { segments: Segment[]; mixer: MixerState; format: OutputFormat }`
  - `type SavedSegment`, `type SavedEdit` (forma JSON del backend)
  - `segLen(s: Segment): number`
  - `fullClip(durationMs: number): Segment[]`
  - `initialState(durationMs: number): EditState`
  - `fromSaved(saved: SavedEdit | null | undefined, durationMs: number): EditState`
  - `toSaved(state: EditState, enabledOnly?: boolean): SavedEdit`
  - `keptMs(segs: Segment[]): number`
  - `outToSeg(segs: Segment[], outMs: number): { index: number; srcMs: number }`
  - `equalState(a: EditState, b: EditState): boolean`

- [ ] **Step 1: Instalar Vitest y añadir el script**

Run: `pnpm add -D vitest@^3.2.4`

Editar `package.json`, dentro de `"scripts"`, añadir tras `"check:watch"`:

```json
    "test": "vitest run",
```

Crear `vitest.config.ts` en la raíz:

```ts
import { defineConfig } from 'vitest/config';

// Configuración aparte de vite.config.ts: los tests cubren módulos TS puros y no necesitan el
// plugin de SvelteKit, que además arrancaría el servidor de desarrollo de Tauri.
export default defineConfig({
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
```

- [ ] **Step 2: Escribir los tests que fallan**

Crear `src/lib/edit-model.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import {
  DEFAULT_FORMAT,
  DEFAULT_MIXER,
  equalState,
  fromSaved,
  fullClip,
  initialState,
  keptMs,
  outToSeg,
  toSaved,
  type Segment,
} from './edit-model';

const seg = (startMs: number, endMs: number, posMs: number, extra: Partial<Segment> = {}): Segment => ({
  startMs,
  endMs,
  posMs,
  boundStartMs: startMs,
  boundEndMs: endMs,
  disabled: false,
  cropX: 0.5,
  ...extra,
});

describe('fullClip / initialState', () => {
  it('cubre el clip entero con un bloque centrado', () => {
    expect(fullClip(10_000)).toEqual([seg(0, 10_000, 0)]);
  });

  it('parte de mezcla y formato por defecto', () => {
    const s = initialState(10_000);
    expect(s.mixer).toEqual(DEFAULT_MIXER);
    expect(s.format).toEqual(DEFAULT_FORMAT);
  });
});

describe('fromSaved', () => {
  it('sin edición guardada devuelve el estado inicial', () => {
    expect(fromSaved(null, 10_000)).toEqual(initialState(10_000));
  });

  it('una edición antigua sin posición, límites, crop_x ni format se empaqueta en orden', () => {
    const s = fromSaved(
      {
        segments: [
          { start_ms: 0, end_ms: 1000 },
          { start_ms: 2000, end_ms: 3000 },
        ],
        mixer: { sys_vol: 0.5, sys_muted: false, mic_vol: 1, mic_muted: true },
      },
      10_000,
    );
    expect(s.segments).toEqual([seg(0, 1000, 0), seg(2000, 3000, 1000)]);
    expect(s.mixer).toEqual({ sys_vol: 0.5, sys_muted: false, mic_vol: 1, mic_muted: true });
    expect(s.format).toEqual({ kind: 'horizontal' });
  });

  it('ordena por posición y respeta límites, desactivado y encuadre guardados', () => {
    const s = fromSaved(
      {
        segments: [
          { start_ms: 5000, end_ms: 6000, pos_ms: 4000, bound_start_ms: 4000, bound_end_ms: 7000, disabled: true, crop_x: 0.2 },
          { start_ms: 0, end_ms: 1000, pos_ms: 0 },
        ],
      },
      10_000,
    );
    expect(s.segments).toEqual([
      seg(0, 1000, 0),
      seg(5000, 6000, 4000, { boundStartMs: 4000, boundEndMs: 7000, disabled: true, cropX: 0.2 }),
    ]);
  });

  it('limita crop_x a [0, 1]', () => {
    const s = fromSaved({ segments: [{ start_ms: 0, end_ms: 1000, crop_x: 2 }] }, 1000);
    expect(s.segments[0].cropX).toBe(1);
  });

  it('lee un formato vertical y descarta uno desconocido', () => {
    const v = fromSaved({ segments: [{ start_ms: 0, end_ms: 1000 }], format: { kind: 'vertical', fill: 'fit' } }, 1000);
    expect(v.format).toEqual({ kind: 'vertical', fill: 'fit' });
    const bad = fromSaved(
      { segments: [{ start_ms: 0, end_ms: 1000 }], format: { kind: 'square' } as never },
      1000,
    );
    expect(bad.format).toEqual({ kind: 'horizontal' });
  });

  it('una lista de segmentos vacía cae al clip completo', () => {
    expect(fromSaved({ segments: [] }, 5000).segments).toEqual(fullClip(5000));
  });
});

describe('toSaved', () => {
  it('serializa todo y vuelve a leerse igual', () => {
    const state = {
      segments: [seg(0, 1000, 0, { cropX: 0.3 }), seg(2000, 3000, 1500, { disabled: true })],
      mixer: DEFAULT_MIXER,
      format: { kind: 'vertical', fill: 'crop' } as const,
    };
    expect(fromSaved(toSaved(state), 10_000)).toEqual(state);
  });

  it('con enabledOnly omite los bloques desactivados', () => {
    const state = {
      segments: [seg(0, 1000, 0), seg(2000, 3000, 1000, { disabled: true })],
      mixer: DEFAULT_MIXER,
      format: DEFAULT_FORMAT,
    };
    expect(toSaved(state, true).segments).toHaveLength(1);
  });
});

describe('keptMs / outToSeg', () => {
  const segs = [seg(0, 1000, 0), seg(2000, 3000, 1000)];

  it('suma solo los bloques activos', () => {
    expect(keptMs(segs)).toBe(2000);
    expect(keptMs([segs[0], { ...segs[1], disabled: true }])).toBe(1000);
  });

  it('traduce tiempo de salida a bloque y tiempo de origen', () => {
    expect(outToSeg(segs, 500)).toEqual({ index: 0, srcMs: 500 });
    expect(outToSeg(segs, 1500)).toEqual({ index: 1, srcMs: 2500 });
  });

  it('más allá del final se queda en el final del último bloque activo', () => {
    expect(outToSeg(segs, 9000)).toEqual({ index: 1, srcMs: 3000 });
  });

  it('salta los bloques desactivados', () => {
    expect(outToSeg([{ ...segs[0], disabled: true }, segs[1]], 0)).toEqual({ index: 1, srcMs: 2000 });
  });
});

describe('equalState', () => {
  it('compara por contenido', () => {
    expect(equalState(initialState(1000), initialState(1000))).toBe(true);
    expect(equalState(initialState(1000), initialState(2000))).toBe(false);
  });
});
```

- [ ] **Step 3: Ejecutar para ver que falla**

Run: `pnpm test`
Expected: FAIL, `Failed to resolve import "./edit-model"`.

- [ ] **Step 4: Implementar el núcleo**

Crear `src/lib/edit-model.ts`:

```ts
// Estado editable del editor y sus operaciones. Todo es puro e inmutable: cada operación
// recibe segmentos y devuelve una lista nueva (o null si no procede), sin Svelte ni IPC, para
// que el historial pueda guardar copias y los tests no necesiten la app.

export const MIN_SEG_MS = 50;

export type Segment = {
  // Tramo de origen que se muestra.
  startMs: number;
  endMs: number;
  // Borde izquierdo en la línea de tiempo del editor; entre bloques puede haber huecos negros
  // que no se exportan.
  posMs: number;
  // Tamaño máximo del bloque: el material de fuera pertenece a otros bloques.
  boundStartMs: number;
  boundEndMs: number;
  disabled: boolean;
  // Centro horizontal del marco 9:16 en el formato vertical con recorte, de 0 a 1.
  cropX: number;
};

export type MixerState = {
  sys_vol: number;
  sys_muted: boolean;
  mic_vol: number;
  mic_muted: boolean;
};

export const DEFAULT_MIXER: MixerState = { sys_vol: 1, sys_muted: false, mic_vol: 1, mic_muted: false };

export type OutputFormat = { kind: 'horizontal' } | { kind: 'vertical'; fill: 'crop' | 'fit' };

export const DEFAULT_FORMAT: OutputFormat = { kind: 'horizontal' };

export type EditState = { segments: Segment[]; mixer: MixerState; format: OutputFormat };

export type SavedSegment = {
  start_ms: number;
  end_ms: number;
  pos_ms?: number | null;
  bound_start_ms?: number | null;
  bound_end_ms?: number | null;
  disabled?: boolean | null;
  crop_x?: number | null;
};

export type SavedEdit = {
  segments: SavedSegment[];
  mixer?: Partial<MixerState> | null;
  format?: OutputFormat | null;
};

const clamp01 = (x: number) => Math.max(0, Math.min(1, x));

export function segLen(s: Segment): number {
  return s.endMs - s.startMs;
}

export function fullClip(durationMs: number): Segment[] {
  return [
    { startMs: 0, endMs: durationMs, posMs: 0, boundStartMs: 0, boundEndMs: durationMs, disabled: false, cropX: 0.5 },
  ];
}

export function initialState(durationMs: number): EditState {
  return { segments: fullClip(durationMs), mixer: { ...DEFAULT_MIXER }, format: { ...DEFAULT_FORMAT } };
}

function readFormat(f: OutputFormat | null | undefined): OutputFormat {
  if (f?.kind === 'vertical' && (f.fill === 'crop' || f.fill === 'fit')) return { kind: 'vertical', fill: f.fill };
  return { kind: 'horizontal' };
}

// Ediciones guardadas por versiones anteriores no traen posición, límites, encuadre ni formato:
// se empaquetan en orden, su rango actual se toma como tamaño máximo y el encuadre va centrado.
export function fromSaved(saved: SavedEdit | null | undefined, durationMs: number): EditState {
  const base = initialState(durationMs);
  if (!saved) return base;
  const mixer = { ...DEFAULT_MIXER, ...(saved.mixer ?? {}) };
  const format = readFormat(saved.format);
  if (!saved.segments?.length) return { segments: base.segments, mixer, format };
  let acc = 0;
  const segments = saved.segments.map((s) => {
    const posMs = s.pos_ms ?? acc;
    acc = posMs + (s.end_ms - s.start_ms);
    return {
      startMs: s.start_ms,
      endMs: s.end_ms,
      posMs,
      boundStartMs: s.bound_start_ms ?? s.start_ms,
      boundEndMs: s.bound_end_ms ?? s.end_ms,
      disabled: s.disabled ?? false,
      cropX: clamp01(s.crop_x ?? 0.5),
    };
  });
  segments.sort((a, b) => a.posMs - b.posMs);
  return { segments, mixer, format };
}

export function toSaved(state: EditState, enabledOnly = false): SavedEdit {
  const list = enabledOnly ? state.segments.filter((s) => !s.disabled) : state.segments;
  return {
    segments: list.map((s) => ({
      start_ms: s.startMs,
      end_ms: s.endMs,
      pos_ms: s.posMs,
      bound_start_ms: s.boundStartMs,
      bound_end_ms: s.boundEndMs,
      disabled: s.disabled,
      crop_x: s.cropX,
    })),
    mixer: { ...state.mixer },
    format: state.format,
  };
}

export function keptMs(segs: Segment[]): number {
  return segs.reduce((a, s) => a + (s.disabled ? 0 : segLen(s)), 0);
}

// La salida concatena los bloques activos en orden, sin huecos. Más allá del final devuelve el
// final del último bloque activo, para que el cabezal nunca apunte a nada.
export function outToSeg(segs: Segment[], outMs: number): { index: number; srcMs: number } {
  let acc = 0;
  let last = -1;
  for (let i = 0; i < segs.length; i++) {
    const s = segs[i];
    if (s.disabled) continue;
    last = i;
    const d = segLen(s);
    if (outMs < acc + d) return { index: i, srcMs: s.startMs + Math.max(0, Math.min(outMs - acc, d)) };
    acc += d;
  }
  if (last >= 0) return { index: last, srcMs: segs[last].endMs };
  return { index: 0, srcMs: segs[0]?.startMs ?? 0 };
}

// Las listas son pequeñas: comparar por JSON es suficiente y evita un comparador a mano que
// habría que actualizar con cada campo nuevo.
export function equalState(a: EditState, b: EditState): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
```

- [ ] **Step 5: Ejecutar y ver que pasa**

Run: `pnpm test`
Expected: PASS, todos los tests de `edit-model.test.ts`.

Run: `pnpm check`
Expected: `0 ERRORS` (los 2 avisos previos de `+layout.svelte` siguen).

- [ ] **Step 6: Commit**

```bash
git add package.json pnpm-lock.yaml vitest.config.ts src/lib/edit-model.ts src/lib/edit-model.test.ts
git commit -F - <<'EOF'
Editor: pure edit model with backward-compatible load and save

First piece of the editor redesign. The editable state (segments, mix
and output format) moves into a module with no Svelte or IPC, so undo
can keep plain copies and the logic can be tested without the app.
Segments gain a per-block crop position and the edit an output format;
both are optional when reading, so edits saved by earlier versions load
centred and horizontal.

Adds Vitest as a dev dependency with its own config, since these tests
do not need the SvelteKit plugin.
EOF
```

---

### Task 2: Operaciones sobre bloques (cortar, recortar, quitar, desactivar, encuadre, ordenar)

**Files:**
- Modify: `src/lib/edit-model.ts` (añadir al final)
- Test: `src/lib/edit-model.test.ts` (añadir al final)

**Interfaces:**
- Consumes: `Segment`, `MIN_SEG_MS`, `segLen` (Task 1).
- Produces:
  - `cutAt(segs: Segment[], index: number, srcMs: number): Segment[] | null`
  - `trim(segs: Segment[], index: number, edge: 'start' | 'end', srcMs: number): Segment[]`
  - `removeAt(segs: Segment[], index: number): Segment[] | null`
  - `toggleDisabled(segs: Segment[], index: number): Segment[]`
  - `setCrop(segs: Segment[], index: number, cropX: number): Segment[]`
  - `sortByPos(segs: Segment[]): Segment[]`

- [ ] **Step 1: Escribir los tests que fallan**

Añadir al import de `src/lib/edit-model.test.ts`: `cutAt, removeAt, setCrop, sortByPos, toggleDisabled, trim`. Añadir al final:

```ts
describe('cutAt', () => {
  it('parte en dos bloques contiguos que fijan su propio rango como límite', () => {
    const out = cutAt([seg(0, 10_000, 0, { cropX: 0.2 })], 0, 4000);
    expect(out).toEqual([
      seg(0, 4000, 0, { cropX: 0.2 }),
      seg(4000, 10_000, 4000, { cropX: 0.2 }),
    ]);
  });

  it('no crea bloques más cortos que MIN_SEG_MS', () => {
    expect(cutAt(fullClip(10_000), 0, 30)).toBeNull();
    expect(cutAt(fullClip(10_000), 0, 9980)).toBeNull();
  });

  it('un índice inexistente no hace nada', () => {
    expect(cutAt(fullClip(10_000), 3, 4000)).toBeNull();
  });
});

describe('trim', () => {
  const cut = [seg(0, 4000, 0), seg(4000, 10_000, 4000)];

  it('el final no pasa del límite del bloque', () => {
    expect(trim(cut, 0, 'end', 3000)[0].endMs).toBe(3000);
    expect(trim(cut, 0, 'end', 5000)[0].endMs).toBe(4000);
  });

  it('el final respeta la longitud mínima', () => {
    expect(trim(cut, 0, 'end', 10)[0].endMs).toBe(MIN_SEG_MS);
  });

  it('recortar el inicio mueve también la posición para fijar el borde derecho', () => {
    const out = trim(cut, 1, 'start', 5000);
    expect(out[1].startMs).toBe(5000);
    expect(out[1].posMs).toBe(5000);
  });

  it('el inicio no crece por encima del bloque anterior', () => {
    const trimmed = trim(cut, 1, 'start', 5000);
    const back = trim(trimmed, 1, 'start', 0);
    expect(back[1].startMs).toBe(4000);
    expect(back[1].posMs).toBe(4000);
  });

  it('no modifica la lista recibida', () => {
    trim(cut, 0, 'end', 3000);
    expect(cut[0].endMs).toBe(4000);
  });
});

describe('removeAt / toggleDisabled / setCrop / sortByPos', () => {
  const two = [seg(0, 1000, 0), seg(1000, 2000, 1000)];

  it('quita un bloque pero nunca el último', () => {
    expect(removeAt(two, 0)).toEqual([seg(1000, 2000, 1000)]);
    expect(removeAt([seg(0, 1000, 0)], 0)).toBeNull();
  });

  it('alterna el desactivado', () => {
    expect(toggleDisabled(two, 1)[1].disabled).toBe(true);
    expect(toggleDisabled(toggleDisabled(two, 1), 1)[1].disabled).toBe(false);
  });

  it('limita el encuadre a [0, 1]', () => {
    expect(setCrop(two, 0, 1.4)[0].cropX).toBe(1);
    expect(setCrop(two, 0, -1)[0].cropX).toBe(0);
    expect(setCrop(two, 0, 0.25)[0].cropX).toBe(0.25);
  });

  it('ordena por posición en la timeline', () => {
    expect(sortByPos([two[1], two[0]])).toEqual(two);
  });
});
```

Añadir también `MIN_SEG_MS` al import.

- [ ] **Step 2: Ejecutar para ver que falla**

Run: `pnpm test`
Expected: FAIL, `cutAt is not a function` (o import no encontrado).

- [ ] **Step 3: Implementar**

Añadir al final de `src/lib/edit-model.ts`:

```ts
// Las dos mitades quedan contiguas y cada una fija su propio rango como tamaño máximo: el
// material del otro lado del corte ya pertenece a la otra mitad.
export function cutAt(segs: Segment[], index: number, srcMs: number): Segment[] | null {
  const s = segs[index];
  if (!s) return null;
  if (srcMs - s.startMs < MIN_SEG_MS || s.endMs - srcMs < MIN_SEG_MS) return null;
  const left: Segment = { ...s, endMs: srcMs, boundStartMs: s.startMs, boundEndMs: srcMs };
  const right: Segment = {
    ...s,
    startMs: srcMs,
    posMs: s.posMs + (srcMs - s.startMs),
    boundStartMs: srcMs,
    boundEndMs: s.endMs,
  };
  return [...segs.slice(0, index), left, right, ...segs.slice(index + 1)];
}

// Acotado a [boundStartMs, boundEndMs] y a MIN_SEG_MS. Al recortar el inicio la posición se
// mueve con él para que el borde derecho no se desplace, y no puede crecer por encima del
// bloque anterior (la lista va ordenada por posición).
export function trim(segs: Segment[], index: number, edge: 'start' | 'end', srcMs: number): Segment[] {
  const s = segs[index];
  if (!s) return segs;
  const next = { ...s };
  if (edge === 'start') {
    const prev = segs[index - 1];
    const leftLimitPos = prev ? prev.posMs + segLen(prev) : 0;
    const minStartByPos = s.startMs + (leftLimitPos - s.posMs);
    const start = Math.max(s.boundStartMs, minStartByPos, Math.min(srcMs, s.endMs - MIN_SEG_MS));
    next.posMs = Math.max(0, s.posMs + (start - s.startMs));
    next.startMs = start;
  } else {
    next.endMs = Math.min(s.boundEndMs, Math.max(srcMs, s.startMs + MIN_SEG_MS));
  }
  return segs.map((x, i) => (i === index ? next : x));
}

export function removeAt(segs: Segment[], index: number): Segment[] | null {
  if (segs.length <= 1 || !segs[index]) return null;
  return segs.filter((_, i) => i !== index);
}

export function toggleDisabled(segs: Segment[], index: number): Segment[] {
  return segs.map((s, i) => (i === index ? { ...s, disabled: !s.disabled } : s));
}

export function setCrop(segs: Segment[], index: number, cropX: number): Segment[] {
  return segs.map((s, i) => (i === index ? { ...s, cropX: clamp01(cropX) } : s));
}

// La salida se reproduce y exporta en orden de lista: ordenar por posición hace que coincida con
// lo que se ve en la timeline.
export function sortByPos(segs: Segment[]): Segment[] {
  return [...segs].sort((a, b) => a.posMs - b.posMs);
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/edit-model.ts src/lib/edit-model.test.ts
git commit -F - <<'EOF'
Editor: block operations in the edit model

Cut, trim, remove, disable, crop and sort as pure functions over a
segment list. Behaviour matches the current editor: halves of a cut fix
their own range as their maximum size, trimming the start moves the
position so the right edge stays put and cannot grow over the previous
block, and the last block can never be removed. Cutting keeps the crop
position on both halves.
EOF
```

---

### Task 3: Imanes, mover bloques, recortar por posición y quitar rango

**Files:**
- Modify: `src/lib/edit-model.ts` (añadir al final)
- Test: `src/lib/edit-model.test.ts` (añadir al final)

**Interfaces:**
- Consumes: `Segment`, `segLen`, `MIN_SEG_MS`, `trim`, `sortByPos` (Tasks 1-2).
- Produces:
  - `snap(value: number, targets: number[], snapMs: number): { value: number; at: number | null }`
  - `snapTargets(segs: Segment[], exclude: number, playheadPos: number): number[]`
  - `moveTo(segs: Segment[], index: number, desiredPos: number, extentMs: number, snapMs: number, targets?: number[]): { segments: Segment[]; snappedAt: number | null }`
  - `trimToPos(segs: Segment[], index: number, edge: 'start' | 'end', pos: number): Segment[]`
  - `removeRange(segs: Segment[], fromPos: number, toPos: number): Segment[] | null`

Nota para el plan 2: `snappedAt` y `snap().at` son posiciones de timeline donde dibujar la línea guía; `snapMs` lo calcula la UI como `8 px` convertidos a ms con el zoom actual, y `0` cuando Alt está pulsado.

- [ ] **Step 1: Escribir los tests que fallan**

Añadir al import: `moveTo, removeRange, snap, snapTargets, trimToPos`. Añadir al final:

```ts
describe('snap', () => {
  it('se pega al objetivo más cercano dentro del umbral', () => {
    expect(snap(1040, [0, 1000, 2000], 50)).toEqual({ value: 1000, at: 1000 });
  });

  it('fuera del umbral no se mueve', () => {
    expect(snap(1100, [0, 1000, 2000], 50)).toEqual({ value: 1100, at: null });
  });

  it('con umbral 0 (Alt) queda desactivado', () => {
    expect(snap(1001, [1000], 0)).toEqual({ value: 1001, at: null });
  });
});

describe('snapTargets', () => {
  it('incluye inicio, cabezal y bordes de los demás bloques', () => {
    const segs = [seg(0, 1000, 0), seg(1000, 2000, 3000)];
    expect(snapTargets(segs, 1, 500).sort((a, b) => a - b)).toEqual([0, 0, 500, 1000]);
  });
});

describe('moveTo', () => {
  // A en [0, 1000], B de 1000 ms en 3000: el hueco libre para B es [1000, fin].
  const segs = [seg(0, 1000, 0), seg(1000, 2000, 3000)];

  it('coloca el bloque donde se pide si cabe', () => {
    const r = moveTo(segs, 1, 1500, 10_000, 0);
    expect(r.segments[1].posMs).toBe(1500);
    expect(r.snappedAt).toBeNull();
  });

  it('se pega al borde del hueco dentro del umbral', () => {
    const r = moveTo(segs, 1, 1080, 10_000, 100);
    expect(r.segments[1].posMs).toBe(1000);
    expect(r.snappedAt).toBe(1000);
  });

  it('puede pegar su borde derecho a un objetivo', () => {
    const r = moveTo(segs, 1, 1550, 10_000, 100, [2600]);
    expect(r.segments[1].posMs).toBe(1600);
    expect(r.snappedAt).toBe(2600);
  });

  it('nunca se solapa: salta al hueco libre más cercano', () => {
    const r = moveTo(segs, 0, 3200, 10_000, 0);
    expect(r.segments[0].posMs).toBe(4000);
  });
});

describe('trimToPos', () => {
  it('convierte la posición de timeline en tiempo de origen', () => {
    const moved = [seg(4000, 10_000, 6000)];
    expect(trimToPos(moved, 0, 'end', 9000)[0].endMs).toBe(7000);
    expect(trimToPos(moved, 0, 'start', 7000)[0].startMs).toBe(5000);
  });
});

describe('removeRange', () => {
  it('quita un tramo del medio y cierra el hueco', () => {
    expect(removeRange(fullClip(10_000), 2000, 5000)).toEqual([
      seg(0, 2000, 0),
      seg(5000, 10_000, 2000),
    ]);
  });

  it('abarca varios bloques', () => {
    const segs = [seg(0, 4000, 0), seg(4000, 10_000, 4000)];
    expect(removeRange(segs, 3000, 6000)).toEqual([seg(0, 3000, 0), seg(6000, 10_000, 3000)]);
  });

  it('desplaza a la izquierda lo que queda detrás aunque el rango caiga en un hueco', () => {
    const segs = [seg(0, 1000, 0), seg(1000, 2000, 5000)];
    expect(removeRange(segs, 2000, 3000)).toEqual([seg(0, 1000, 0), seg(1000, 2000, 4000)]);
  });

  it('descarta restos más cortos que MIN_SEG_MS', () => {
    expect(removeRange(fullClip(10_000), 30, 5000)).toEqual([seg(5000, 10_000, 30)]);
  });

  it('no deja la edición vacía', () => {
    expect(removeRange(fullClip(10_000), 0, 10_000)).toBeNull();
    expect(removeRange(fullClip(10_000), 0, 9980)).toBeNull();
  });

  it('un rango vacío no hace nada', () => {
    expect(removeRange(fullClip(10_000), 3000, 3000)).toBeNull();
  });
});
```

- [ ] **Step 2: Ejecutar para ver que falla**

Run: `pnpm test`
Expected: FAIL, `snap is not a function`.

- [ ] **Step 3: Implementar**

Añadir al final de `src/lib/edit-model.ts`:

```ts
// El umbral llega ya en ms (la UI convierte 8 px con el zoom actual) para que el imán se sienta
// igual con cualquier zoom. snapMs <= 0 lo desactiva (Alt pulsado).
export function snap(value: number, targets: number[], snapMs: number): { value: number; at: number | null } {
  if (snapMs <= 0) return { value, at: null };
  let best: number | null = null;
  let bestDist = snapMs;
  for (const t of targets) {
    const d = Math.abs(t - value);
    if (d <= bestDist) {
      bestDist = d;
      best = t;
    }
  }
  return best === null ? { value, at: null } : { value: best, at: best };
}

export function snapTargets(segs: Segment[], exclude: number, playheadPos: number): number[] {
  const out = [0, playheadPos];
  segs.forEach((s, i) => {
    if (i !== exclude) out.push(s.posMs, s.posMs + segLen(s));
  });
  return out;
}

// Coloca el bloque en el hueco libre más cercano a donde se pide, sin solaparse nunca con otro.
// Dentro de ese hueco puede pegarse a sus bordes o, con cualquiera de sus dos bordes, a los
// objetivos recibidos (cabezal, bordes de otros bloques).
export function moveTo(
  segs: Segment[],
  index: number,
  desiredPos: number,
  extentMs: number,
  snapMs: number,
  targets: number[] = [],
): { segments: Segment[]; snappedAt: number | null } {
  const cur = segs[index];
  if (!cur) return { segments: segs, snappedAt: null };
  const dur = segLen(cur);
  const occ = segs
    .filter((_, i) => i !== index)
    .map((s) => ({ a: s.posMs, b: s.posMs + segLen(s) }))
    .sort((x, y) => x.a - y.a);
  const gaps: [number, number][] = [];
  let cursor = 0;
  for (const o of occ) {
    if (o.a - cursor > 0.5) gaps.push([cursor, o.a]);
    cursor = Math.max(cursor, o.b);
  }
  gaps.push([cursor, Math.max(cursor, extentMs)]);

  let best: { pos: number; lo: number; hi: number } | null = null;
  let bestDist = Infinity;
  for (const [gs, ge] of gaps) {
    if (ge - gs < dur - 0.5) continue;
    const lo = gs;
    const hi = ge - dur;
    const pos = Math.max(lo, Math.min(desiredPos, hi));
    const dist = Math.abs(pos - desiredPos);
    if (dist < bestDist) {
      bestDist = dist;
      best = { pos, lo, hi };
    }
  }
  if (!best) return { segments: segs, snappedAt: null };

  let pos = best.pos;
  let snappedAt: number | null = null;
  if (snapMs > 0) {
    const cands: { pos: number; edge: number }[] = [
      { pos: best.lo, edge: best.lo },
      { pos: best.hi, edge: best.hi + dur },
    ];
    for (const t of targets) cands.push({ pos: t, edge: t }, { pos: t - dur, edge: t });
    let d = snapMs;
    for (const c of cands) {
      if (c.pos < best.lo - 0.5 || c.pos > best.hi + 0.5) continue;
      const dist = Math.abs(c.pos - best.pos);
      if (dist <= d) {
        d = dist;
        pos = c.pos;
        snappedAt = c.edge;
      }
    }
  }
  return { segments: segs.map((s, i) => (i === index ? { ...s, posMs: pos } : s)), snappedAt };
}

// La UI arrastra bordes en posiciones de timeline (donde viven los imanes); el recorte trabaja en
// tiempo de origen. La traducción es la misma para los dos bordes.
export function trimToPos(segs: Segment[], index: number, edge: 'start' | 'end', pos: number): Segment[] {
  const s = segs[index];
  if (!s) return segs;
  return trim(segs, index, edge, s.startMs + (pos - s.posMs));
}

// Quita [fromPos, toPos) de la timeline y cierra el hueco: lo que queda detrás se desplaza a la
// izquierda. Los trozos que sobreviven a un corte fijan su propio rango como límite, igual que
// cutAt, y los que quedarían por debajo de MIN_SEG_MS se descartan.
export function removeRange(segs: Segment[], fromPos: number, toPos: number): Segment[] | null {
  const w = toPos - fromPos;
  if (w < 1) return null;
  const out: Segment[] = [];
  for (const s of segs) {
    const a = s.posMs;
    const b = a + segLen(s);
    if (b <= fromPos) {
      out.push(s);
      continue;
    }
    if (a >= toPos) {
      out.push({ ...s, posMs: a - w });
      continue;
    }
    if (a < fromPos) {
      const cut = s.startMs + (fromPos - a);
      if (cut - s.startMs >= MIN_SEG_MS) out.push({ ...s, endMs: cut, boundStartMs: s.startMs, boundEndMs: cut });
    }
    if (b > toPos) {
      const cut = s.startMs + (toPos - a);
      if (s.endMs - cut >= MIN_SEG_MS) {
        out.push({ ...s, startMs: cut, posMs: fromPos, boundStartMs: cut, boundEndMs: s.endMs });
      }
    }
  }
  return out.length === 0 ? null : sortByPos(out);
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/edit-model.ts src/lib/edit-model.test.ts
git commit -F - <<'EOF'
Editor: snapping, block moves and range removal in the edit model

Snapping takes its threshold in milliseconds, so the UI can convert a
fixed 8px at the current zoom and pass 0 while Alt is held. Moving a
block keeps the current rule of landing in the nearest free gap without
ever overlapping, and can now also stick either edge to the playhead or
to other blocks. trimToPos lets the UI drag edges in timeline positions,
where the snap targets live.

Removing a range cuts at both ends, drops what lies inside and closes
the gap by shifting everything after it left. Leftovers shorter than the
minimum block are dropped, and a removal that would leave nothing is
refused.
EOF
```

---

### Task 4: Historial de deshacer / rehacer

**Files:**
- Create: `src/lib/edit-history.ts`
- Test: `src/lib/edit-history.test.ts`

**Interfaces:**
- Produces:
  - `class EditHistory<T>` con `constructor(limit?: number, equal?: (a: T, b: T) => boolean)`
  - `record(before: T, after: T): void` — registra un paso si `before` y `after` difieren; vacía rehacer.
  - `undo(current: T): T | null`
  - `redo(current: T): T | null`
  - `canUndo: boolean`, `canRedo: boolean` (getters)
  - `clear(): void`

Uso previsto en el plan 2: una operación discreta llama `record(antes, despues)` al aplicarse; un gesto continuo toma `antes` al pulsar y llama `record` una sola vez al soltar. El llamante pasa copias planas (`$state.snapshot(...)`), nunca proxies de Svelte.

- [ ] **Step 1: Escribir los tests que fallan**

Crear `src/lib/edit-history.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { EditHistory } from './edit-history';

describe('EditHistory', () => {
  it('deshace y rehace volviendo al estado exacto', () => {
    const h = new EditHistory<number>();
    h.record(1, 2);
    h.record(2, 3);
    expect(h.undo(3)).toBe(2);
    expect(h.undo(2)).toBe(1);
    expect(h.undo(1)).toBeNull();
    expect(h.redo(1)).toBe(2);
    expect(h.redo(2)).toBe(3);
    expect(h.redo(3)).toBeNull();
  });

  it('un gesto que no cambia nada no se registra', () => {
    const h = new EditHistory<number>();
    h.record(5, 5);
    expect(h.canUndo).toBe(false);
  });

  it('un gesto largo registrado al soltar es un solo paso', () => {
    const h = new EditHistory<number>();
    const before = 10;
    // durante el arrastre el estado pasa por 11, 12, 13 sin registrarse
    h.record(before, 13);
    expect(h.undo(13)).toBe(10);
    expect(h.canUndo).toBe(false);
  });

  it('un paso nuevo vacía lo que se podía rehacer', () => {
    const h = new EditHistory<number>();
    h.record(1, 2);
    h.undo(2);
    h.record(1, 7);
    expect(h.canRedo).toBe(false);
  });

  it('respeta el límite descartando lo más antiguo', () => {
    const h = new EditHistory<number>(3);
    for (let i = 0; i < 5; i++) h.record(i, i + 1);
    expect(h.undo(5)).toBe(4);
    expect(h.undo(4)).toBe(3);
    expect(h.undo(3)).toBe(2);
    expect(h.undo(2)).toBeNull();
  });

  it('compara objetos por contenido por defecto', () => {
    const h = new EditHistory<{ a: number }>();
    h.record({ a: 1 }, { a: 1 });
    expect(h.canUndo).toBe(false);
  });

  it('clear lo vacía todo', () => {
    const h = new EditHistory<number>();
    h.record(1, 2);
    h.undo(2);
    h.clear();
    expect(h.canUndo).toBe(false);
    expect(h.canRedo).toBe(false);
  });
});
```

- [ ] **Step 2: Ejecutar para ver que falla**

Run: `pnpm test`
Expected: FAIL, `Failed to resolve import "./edit-history"`.

- [ ] **Step 3: Implementar**

Crear `src/lib/edit-history.ts`:

```ts
// Historial por copias completas en vez de operaciones inversas: el estado del editor es
// pequeño, copiarlo es barato, y deshacer devuelve exactamente lo que había sin depender de que
// cada operación sepa revertirse.
export class EditHistory<T> {
  private past: T[] = [];
  private future: T[] = [];

  constructor(
    private readonly limit = 100,
    private readonly equal: (a: T, b: T) => boolean = (a, b) => JSON.stringify(a) === JSON.stringify(b),
  ) {}

  // Un gesto continuo (arrastrar un borde, deslizar un volumen) se registra una sola vez al
  // soltar, con el estado de antes de empezar: si no, deshacer iría milímetro a milímetro.
  record(before: T, after: T): void {
    if (this.equal(before, after)) return;
    this.past.push(before);
    if (this.past.length > this.limit) this.past.shift();
    this.future = [];
  }

  undo(current: T): T | null {
    const prev = this.past.pop();
    if (prev === undefined) return null;
    this.future.push(current);
    return prev;
  }

  redo(current: T): T | null {
    const next = this.future.pop();
    if (next === undefined) return null;
    this.past.push(current);
    return next;
  }

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  clear(): void {
    this.past = [];
    this.future = [];
  }
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lib/edit-history.ts src/lib/edit-history.test.ts
git commit -F - <<'EOF'
Editor: undo and redo history

Keeps full copies of the editable state rather than inverse operations:
the state is small, copying it is cheap, and undo lands exactly where
the user was without every operation having to know how to revert
itself. A continuous gesture is recorded once, on release, against the
state from before it started, and an unchanged gesture records nothing.
Capped at 100 steps.
EOF
```

---

### Task 5: Tabla de atajos

**Files:**
- Create: `src/lib/shortcuts.ts`
- Test: `src/lib/shortcuts.test.ts`

**Interfaces:**
- Produces:
  - `type ActionId = 'play' | 'prevFrame' | 'nextFrame' | 'cut' | 'remove' | 'toggleDisable' | 'undo' | 'redo' | 'fullscreen' | 'reset'`
  - `type KeyCombo = { key: string; ctrl?: boolean; shift?: boolean }`
  - `type Shortcut = { action: ActionId; combos: KeyCombo[]; label: string }` (`label` es la clave i18n que el plan 2 añade a `i18n.svelte.ts`)
  - `SHORTCUTS: Shortcut[]` (en el orden en que se muestran en el menú)
  - `matchShortcut(e: { key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }): ActionId | null`
  - `comboTokens(c: KeyCombo): string[]` — tokens para mostrar: `'Ctrl'`, `'Shift'`, `'Space'`, `'Del'`, `'Backspace'`, `'←'`, `'→'` o la letra en mayúscula. El plan 2 traduce `'Space'` y `'Del'` con i18n.

- [ ] **Step 1: Escribir los tests que fallan**

Crear `src/lib/shortcuts.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { SHORTCUTS, comboTokens, matchShortcut } from './shortcuts';

const ev = (key: string, mods: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }> = {}) => ({
  key,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

describe('matchShortcut', () => {
  it('reconoce las teclas simples sin importar mayúsculas', () => {
    expect(matchShortcut(ev('c'))).toBe('cut');
    expect(matchShortcut(ev('C'))).toBe('cut');
    expect(matchShortcut(ev(' '))).toBe('play');
    expect(matchShortcut(ev('k'))).toBe('play');
    expect(matchShortcut(ev('d'))).toBe('toggleDisable');
    expect(matchShortcut(ev('Delete'))).toBe('remove');
    expect(matchShortcut(ev('Backspace'))).toBe('remove');
    expect(matchShortcut(ev('ArrowLeft'))).toBe('prevFrame');
    expect(matchShortcut(ev('ArrowRight'))).toBe('nextFrame');
    expect(matchShortcut(ev('f'))).toBe('fullscreen');
  });

  it('distingue deshacer de rehacer por los modificadores', () => {
    expect(matchShortcut(ev('z', { ctrlKey: true }))).toBe('undo');
    expect(matchShortcut(ev('Z', { ctrlKey: true, shiftKey: true }))).toBe('redo');
    expect(matchShortcut(ev('y', { ctrlKey: true }))).toBe('redo');
  });

  it('un modificador de más no dispara la tecla simple', () => {
    expect(matchShortcut(ev('c', { ctrlKey: true }))).toBeNull();
    expect(matchShortcut(ev('c', { shiftKey: true }))).toBeNull();
  });

  it('Alt y Meta nunca son atajos del editor', () => {
    expect(matchShortcut(ev('c', { altKey: true }))).toBeNull();
    expect(matchShortcut(ev('z', { ctrlKey: true, metaKey: true }))).toBeNull();
  });

  it('una tecla sin atajo no hace nada', () => {
    expect(matchShortcut(ev('q'))).toBeNull();
  });
});

describe('SHORTCUTS', () => {
  it('cada acción aparece una sola vez y tiene etiqueta', () => {
    const actions = SHORTCUTS.map((s) => s.action);
    expect(new Set(actions).size).toBe(actions.length);
    for (const s of SHORTCUTS) expect(s.label).toMatch(/^ed\.act\./);
  });

  it('restablecer no tiene atajo', () => {
    expect(SHORTCUTS.find((s) => s.action === 'reset')?.combos).toEqual([]);
  });
});

describe('comboTokens', () => {
  it('da los tokens en orden de lectura', () => {
    expect(comboTokens({ key: 'z', ctrl: true, shift: true })).toEqual(['Ctrl', 'Shift', 'Z']);
    expect(comboTokens({ key: ' ' })).toEqual(['Space']);
    expect(comboTokens({ key: 'Delete' })).toEqual(['Del']);
    expect(comboTokens({ key: 'ArrowLeft' })).toEqual(['←']);
  });
});
```

- [ ] **Step 2: Ejecutar para ver que falla**

Run: `pnpm test`
Expected: FAIL, `Failed to resolve import "./shortcuts"`.

- [ ] **Step 3: Implementar**

Crear `src/lib/shortcuts.ts`:

```ts
// Tabla única de atajos del editor: la lee el manejador de teclado y la pinta el menú de
// herramientas, así lo que el menú anuncia es siempre lo que el teclado hace.

export type ActionId =
  | 'play'
  | 'prevFrame'
  | 'nextFrame'
  | 'cut'
  | 'remove'
  | 'toggleDisable'
  | 'undo'
  | 'redo'
  | 'fullscreen'
  | 'reset';

// key es event.key normalizado: letras en minúscula, teclas con nombre tal cual.
export type KeyCombo = { key: string; ctrl?: boolean; shift?: boolean };

export type Shortcut = { action: ActionId; combos: KeyCombo[]; label: string };

export const SHORTCUTS: Shortcut[] = [
  { action: 'play', combos: [{ key: ' ' }, { key: 'k' }], label: 'ed.act.play' },
  { action: 'prevFrame', combos: [{ key: 'ArrowLeft' }], label: 'ed.act.prevFrame' },
  { action: 'nextFrame', combos: [{ key: 'ArrowRight' }], label: 'ed.act.nextFrame' },
  { action: 'cut', combos: [{ key: 'c' }], label: 'ed.act.cut' },
  { action: 'remove', combos: [{ key: 'Delete' }, { key: 'Backspace' }], label: 'ed.act.remove' },
  { action: 'toggleDisable', combos: [{ key: 'd' }], label: 'ed.act.toggleDisable' },
  { action: 'undo', combos: [{ key: 'z', ctrl: true }], label: 'ed.act.undo' },
  { action: 'redo', combos: [{ key: 'z', ctrl: true, shift: true }, { key: 'y', ctrl: true }], label: 'ed.act.redo' },
  { action: 'fullscreen', combos: [{ key: 'f' }], label: 'ed.act.fullscreen' },
  // Sin atajo a propósito: es destructivo y raro, se llega solo desde el menú.
  { action: 'reset', combos: [], label: 'ed.act.reset' },
];

function normalizeKey(key: string): string {
  return key.length === 1 ? key.toLowerCase() : key;
}

// Los modificadores tienen que coincidir exactos: Ctrl+C no es cortar. Alt queda libre para
// desactivar los imanes al arrastrar, y Meta no tiene atajos en Windows.
export function matchShortcut(e: {
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}): ActionId | null {
  if (e.altKey || e.metaKey) return null;
  const key = normalizeKey(e.key);
  for (const s of SHORTCUTS) {
    for (const c of s.combos) {
      if (c.key === key && !!c.ctrl === e.ctrlKey && !!c.shift === e.shiftKey) return s.action;
    }
  }
  return null;
}

const NAMED: Record<string, string> = {
  ' ': 'Space',
  Delete: 'Del',
  Backspace: 'Backspace',
  ArrowLeft: '←',
  ArrowRight: '→',
};

export function comboTokens(c: KeyCombo): string[] {
  const out: string[] = [];
  if (c.ctrl) out.push('Ctrl');
  if (c.shift) out.push('Shift');
  out.push(NAMED[c.key] ?? c.key.toUpperCase());
  return out;
}
```

- [ ] **Step 4: Ejecutar y ver que pasa**

Run: `pnpm test`
Expected: PASS, todos los ficheros de test.

Run: `pnpm check`
Expected: `0 ERRORS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/shortcuts.ts src/lib/shortcuts.test.ts
git commit -F - <<'EOF'
Editor: single shortcut table for keyboard and tools menu

Both the key handler and the upcoming tools menu read the same table,
so what the menu lists is always what the keyboard does. Modifiers must
match exactly (Ctrl+C is not cut), Alt is left free to disable snapping
while dragging, and reset deliberately has no key.
EOF
```

---

## Cierre del plan

Al terminar, `pnpm test` pasa con los tres ficheros de test, `pnpm check` sigue en 0 errores, y la app no cambia de comportamiento (el editor actual sigue en uso). El plan 2 (editor nuevo) consume exactamente las interfaces declaradas arriba.
