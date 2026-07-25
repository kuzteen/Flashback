# Compartir clips por arrastre nativo

## Objetivo

El botón de compartir abre un popup con el clip en pantalla que se puede **arrastrar a
cualquier otra aplicación** (Discord, WhatsApp, el Explorador). El popup ofrece además
presets de tamaño de archivo para clips que no caben en el límite de subida del destino.

No hay subida, ni cuentas, ni integración con la API de ninguna plataforma: el arrastre es
local y el destino lo recibe como un archivo normal.

## Restricción técnica de partida

WebView2 no puede arrastrar un archivo real fuera de la ventana. El HTML5 drag-and-drop no
produce `CF_HDROP`, que es lo que Discord y el Explorador esperan de una operación OLE. El
arrastre tiene que originarse en Rust mediante `DoDragDrop`. Esto no es una preferencia de
diseño: es la única vía que funciona.

## Alcance

- Popup genérico de compartir, sin selector de plataforma ni marcas de terceros.
- Disponible desde la tarjeta de clip (biblioteca, juegos, favoritos) y desde el editor.
- Presets de tamaño: Original, 10 MB, 50 MB, 100 MB.
- Escalado automático de resolución cuando el bitrate objetivo no da para la resolución actual.

Fuera de alcance: subida a servicios, historial de compartidos, enlaces, portapapeles.

---

## 1. Arrastre nativo (`src-tauri/src/dragdrop.rs`)

Módulo nuevo, solo Windows. **Sin dependencias nuevas**: las features `Win32_UI_Shell`,
`Win32_System_Ole`, `Win32_System_Com` y `Win32_System_Com_StructuredStorage` del crate
`windows` ya están declaradas. Puede hacer falta añadir `Win32_System_Memory` (para
`GlobalAlloc`) y `Win32_Graphics_Gdi` (para el `HBITMAP` de la imagen de arrastre).

### Comando

```rust
#[tauri::command]
async fn start_file_drag(app: AppHandle, path: String, thumb: Option<String>) -> Result<bool, String>
```

Devuelve `true` si el arrastre terminó en un soltado efectivo (`DROPEFFECT_COPY`), para que
el frontend pueda cerrar el popup.

### Secuencia

1. **Salto al hilo principal** con `app.run_on_main_thread`. `DoDragDrop` es modal, requiere
   apartamento STA con OLE inicializado y captura del ratón sobre el hilo de UI. Los comandos
   de Tauri corren en el runtime async (MTA), así que invocarlo directamente falla.
2. Construir el `IDataObject` con `SHCreateDataObject` y rellenar `CF_HDROP`: un `HGLOBAL`
   con la estructura `DROPFILES` (`pFiles` = `size_of::<DROPFILES>()`, `fWide = TRUE`)
   seguida de la ruta en UTF-16 con doble terminador nul.
3. Imagen de arrastre: `IDragSourceHelper2::InitializeFromBitmap` con la miniatura del clip
   como `HBITMAP`, para que bajo el cursor se vea el thumbnail y no un icono genérico. Si la
   miniatura no está disponible, se omite este paso (el shell usa su icono por defecto).
4. `SHDoDragDrop(hwnd, &data_object, None, DROPEFFECT_COPY, &mut effect)`. Al pasar `None`
   como `IDropSource`, el shell aporta la implementación por defecto: no hay que escribirla.

### Impacto en rendimiento

`SHDoDragDrop` bloquea el hilo de UI mientras dura el arrastre. Es el comportamiento normal
de cualquier aplicación nativa de Windows, incluido el Explorador. **No toca el camino
sagrado**: captura, encoder, audio y ring buffer viven en sus propios hilos y siguen
corriendo sin verse afectados.

### Lado frontend

El arrastre se dispara con `pointerdown` y un umbral de ~4 px de movimiento en `pointermove`
antes de invocar el comando, para no lanzar un arrastre en cada clic. El elemento lleva
`draggable="false"` para que el arrastre nativo de WebView2 no compita con el de OLE.

---

## 2. Preparación del archivo (`share_prepare`)

Un único comando decide qué archivo se arrastra. Unifica el caso de la tarjeta y el del
editor sin ramas duplicadas.

```rust
#[tauri::command]
async fn share_prepare(
    app: AppHandle,
    path: String,
    edit: ClipEdit,
    target_bytes: Option<u64>,
    watermark: Option<String>,
) -> Result<String, String>
```

**Camino rápido (coste cero):** si la edición es identidad (un solo segmento que cubre el
clip entero y mezcla por defecto), no hay preset de tamaño y no hay marca de agua, devuelve
`path` sin tocar nada. Es el caso habitual al compartir desde la tarjeta.

**Camino de recodificado:** en cualquier otro caso se materializa un archivo temporal
reutilizando `editor::export_clip`, que sigue siendo el único camino que recodifica.

### Cálculo del bitrate objetivo

```
bits_disponibles = target_bytes * 8 * 0.97      // margen para contenedor y moov
bitrate_video    = (bits_disponibles - bits_audio) / duración_s
```

`bits_audio` sale del bitrate AAC real de la pista. El resultado se acota por abajo a
300 kbps y **nunca se sube por encima del bitrate original**: recodificar hacia arriba solo
engorda el archivo y degrada la imagen.

Si el clip ya pesa menos que el objetivo, el preset se muestra **deshabilitado** en el
popup: no tiene sentido recodificar para acabar con un archivo peor.

### Escalera de resolución

A 1080p60, un clip de un minuto en 10 MB son ~1,2 Mbps: bloques por todas partes. Cuando el
bitrate objetivo cae por debajo del umbral de calidad de la resolución actual, se baja un
peldaño hasta que cuadre:

```
umbral(w, h, fps) = w * h * fps / 60        // bits por segundo
escalera          = 1080 → 720 → 480        // el ancho se deriva manteniendo el aspecto
```

Se aplica ajustando el media type de salida en `do_export`. La resolución final nunca sube
por encima de la del original.

### Cambios en `editor.rs`

`export_clip` recibe dos parámetros opcionales nuevos:

```rust
bitrate: Option<u32>,     // sobreescribe meta.bitrate
max_height: Option<u32>,  // reescala la salida si el original es más alto
```

Cuando ambos son `None` el comportamiento es idéntico al actual, así que la exportación del
editor no cambia. El override se aplica sobre el `meta.bitrate` calculado en
`read_video_meta` (`editor.rs:523`) y sobre `MF_MT_AVG_BITRATE`/`MF_MT_FRAME_SIZE` del media
type de salida (`editor.rs:666`).

### Archivos temporales

- Ubicación: `%LOCALAPPDATA%\Flashback\share\`.
- Nombre: hash de `(ruta, mtime, edición, target_bytes, watermark)` → si vuelves a arrastrar
  el mismo clip con el mismo preset, no se recodifica otra vez.
- Purga al arrancar la aplicación de todo lo que tenga más de 24 horas. Barato y evita que
  el directorio crezca sin control.

### Progreso

Evento propio `share-progress` (no se reutiliza `export-progress`, que ya escucha el editor
y provocaría colisiones si ambos están abiertos).

---

## 3. Interfaz

### `src/lib/components/ShareDialog.svelte` (nuevo)

Modal sobre backdrop, reutilizando el patrón visual de `export-backdrop`/`export-card` del
editor.

```
┌─────────────────────────────────────┐
│  ×              COMPARTIR           │
│                                     │
│  Elige un tamaño de archivo         │
│  ┌────────┬──────┬──────┬───────┐   │
│  │Original│ 10 MB│ 50 MB│ 100 MB│   │
│  └────────┴──────┴──────┴───────┘   │
│                                     │
│  ╭ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ╮   │
│  │  [poster / vídeo]     ⏱ 00:56 │   │
│  │      Arrastrar y soltar       │   │
│  │  Arrastra el clip a cualquier │   │
│  │  app para compartirlo         │   │
│  ╰ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ╯   │
└─────────────────────────────────────┘
```

- La zona de arrastre pasa de borde discontinuo a sólido al hover, con una capa de texto
  encima y cursor `grab` / `grabbing` durante el arrastre.
- El preview reutiliza el mecanismo ya probado de `ClipCard`: `poster` estático y
  `<video muted loop>` montado solo al hover, sin precarga.
- Al pulsar un preset distinto de Original, la zona de arrastre se sustituye por una barra de
  progreso hasta que el archivo está listo.
- Cierre con la `×`, con `Escape` o al hacer clic fuera.

### Estado: `src/lib/share.svelte.ts` (nuevo)

```ts
shareState = { open, clip, edit, preset, preparing, progress, readyPath, error }
```

### Puntos de entrada

- `ClipCard.svelte:202` — el botón share, hoy un stub que solo hace `stopPropagation`. Abre
  el popup con edición identidad y sin marca de agua.
- `Editor.svelte` — botón Compartir junto a Exportar (`Editor.svelte:1168`). Abre el popup
  con la edición actual y el estado del toggle de marca de agua, de modo que se comparte lo
  que se acaba de recortar.

### i18n

Claves nuevas en `src/lib/i18n.svelte.ts` (español e inglés): título, etiqueta del selector
de tamaño, textos de la zona de arrastre, estado de preparación, presets deshabilitados y
mensajes de error.

---

## 4. Manejo de errores

| Situación | Comportamiento |
|---|---|
| El archivo ya no existe en disco | Mensaje en el popup y refresco de la biblioteca |
| Falla el recodificado | Mensaje con el motivo; el preset vuelve a Original |
| El clip ya es menor que el preset | Preset deshabilitado, con tooltip explicativo |
| `SHDoDragDrop` devuelve `DROPEFFECT_NONE` | Sin acción: el popup sigue abierto |
| Falla la miniatura para la imagen de arrastre | Se arrastra igual, con el icono del shell |

## 5. Verificación

- Arrastrar a Discord, a WhatsApp Web, al Explorador y al escritorio: el archivo llega
  completo y reproducible.
- Arrastrar con el preset Original no genera ningún archivo temporal ni recodifica.
- Cada preset produce un archivo por debajo de su límite.
- Un clip que ya pesa menos que el preset muestra ese preset deshabilitado.
- Compartir desde el editor con cortes activos arrastra el montaje, no el original.
- Repetir un arrastre con el mismo preset no vuelve a recodificar (acierta la caché).
- Con una captura activa, el arrastre no provoca cortes ni pérdida de frames en el clip que
  se está grabando.
