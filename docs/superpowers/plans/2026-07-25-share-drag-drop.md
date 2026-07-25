# Compartir por arrastre nativo — Plan de implementación

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** El botón de compartir abre un popup con el clip que se arrastra a cualquier aplicación de Windows, con presets opcionales de tamaño de archivo.

**Architecture:** Un módulo Rust nuevo (`dragdrop.rs`) inicia una operación OLE de arrastre desde el hilo principal usando el `IDataObject` que el shell genera para el archivo. Un comando `share_prepare` decide qué archivo se arrastra: el original cuando no hay nada que aplicar, o un recodificado temporal (reutilizando `editor::export_clip`) cuando hay cortes, marca de agua o preset de tamaño. El frontend añade un `ShareDialog.svelte` accesible desde la tarjeta y desde el editor.

**Tech Stack:** Rust + `windows` 0.61 (Shell/Ole/COM), Media Foundation, Tauri 2, Svelte 5.

## Global Constraints

- **Cero dependencias nuevas.** Todo se hace con features del crate `windows` ya declaradas o añadiendo features al mismo crate.
- **El camino de captura es sagrado.** Nada de este trabajo puede tocar `capture/`. El bloqueo del hilo de UI durante el arrastre es aceptable porque captura, encoder y audio corren en sus propios hilos.
- **La exportación es el único camino que recodifica.** El recodificado de los presets pasa por `editor::export_clip`, no por una ruta paralela.
- **Sin comentarios decorativos.** Solo comentar el *porqué* de lo no obvio (§9 de CLAUDE.md).
- **Commits en inglés, autoría exclusiva del dueño.** Nunca `Co-Authored-By: Claude`.
- **i18n obligatorio.** Toda cadena visible va a `src/lib/i18n.svelte.ts` en español e inglés.

---

### Task 1: Escalado y bitrate opcionales en el export

**Files:**
- Modify: `src-tauri/src/editor.rs` (`export_clip`, `do_export`, `read_video_meta`)
- Modify: `src-tauri/src/lib.rs` (llamada en `export_clip`)

**Interfaces:**
- Produces: `editor::export_clip(src, dst, edit, watermark, bitrate: Option<u32>, max_height: Option<u32>, progress)`

Con `bitrate` y `max_height` en `None` el comportamiento debe ser byte a byte el actual: la exportación del editor no cambia.

- [ ] **Step 1: Añadir los parámetros a la firma**

En `export_clip` y `do_export`, tras `watermark`. Propagar hasta `do_export`.

- [ ] **Step 2: Aplicar el bitrate**

En `do_export`, tras `let meta = read_video_meta(src)`:

```rust
let mut meta = meta;
if let Some(bps) = bitrate {
    meta.bitrate = bps.max(100_000);
}
```

- [ ] **Step 3: Aplicar el escalado en el tipo RGB32 del lector**

El lector ya tiene `MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING`, así que puede entregar
RGB32 ya reescalado sin insertar ningún MFT a mano. Se pide el tamaño objetivo y **se lee de
vuelta el que concedió**: si lo rechaza, se sigue con el nativo en vez de fallar.

```rust
let (req_w, req_h) = match max_height {
    Some(mh) if mh < meta.height => {
        let w = (meta.width as u64 * mh as u64 / meta.height as u64) as u32;
        ((w + 1) & !1, (mh + 1) & !1)
    }
    _ => (meta.width, meta.height),
};
```

`rgb` se construye con `pack2(req_w, req_h)`. Si `SetCurrentMediaType` falla, reintentar con
`pack2(meta.width, meta.height)`. Después, derivar las dimensiones reales de `v_in`:

```rust
let out_size = unsafe { v_in.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(pack2(meta.width, meta.height));
let out_w = (out_size >> 32) as u32;
let out_h = (out_size & 0xFFFF_FFFF) as u32;
```

- [ ] **Step 4: Usar `out_w`/`out_h` aguas abajo**

Sustituir `meta.width`/`meta.height` por `out_w`/`out_h` en: `watermark::Logo::rasterize`,
`v_out.SetUINT64(&MF_MT_FRAME_SIZE, ...)` y las dos llamadas a `blend_watermark`. Es
obligatorio: rasterizar el logo al tamaño de origen sobre un frame reescalado lo dibujaría
fuera del encuadre.

- [ ] **Step 5: Compilar**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: sin errores ni warnings nuevos.

- [ ] **Step 6: Verificar que el export del editor no cambió**

`pnpm tauri dev`, recortar un clip y exportarlo. El archivo sale con la misma resolución y
peso aproximado que antes del cambio.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/editor.rs src-tauri/src/lib.rs
git commit -m "Editor: allow optional bitrate and downscale overrides on export"
```

---

### Task 2: Arrastre nativo OLE

**Files:**
- Create: `src-tauri/src/dragdrop.rs`
- Modify: `src-tauri/src/lib.rs` (declarar módulo, registrar comando)
- Modify: `src-tauri/Cargo.toml` (features `Win32_UI_Shell_Common`, `Win32_System_Com_Marshal` si el compilador las pide)

**Interfaces:**
- Produces: comando Tauri `start_file_drag(path: String) -> Result<bool, String>`

- [ ] **Step 1: Escribir el módulo**

El `IDataObject` lo genera el shell a partir del archivo: trae `CF_HDROP` y el resto de
formatos que espera el Explorador, y `SHDoDragDrop` obtiene de ahí la imagen de arrastre
(la miniatura del vídeo de la caché del shell) sin que haya que construirla.

`SHDoDragDrop` es modal y exige STA con OLE inicializado y captura del ratón: por eso salta
al hilo principal, donde vive el bucle de eventos. Los comandos de Tauri corren en el runtime
async, que es MTA, y ahí la llamada falla.

```rust
#[cfg(target_os = "windows")]
mod win {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE, S_OK};
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{OleInitialize, OleUninitialize, DROPEFFECT, DROPEFFECT_COPY};
    use windows::Win32::UI::Shell::{SHCreateItemFromParsingName, SHDoDragDrop, BHID_DataObject, IShellItem};

    const DRAGDROP_S_DROP: windows::core::HRESULT = windows::core::HRESULT(0x0004_0100u32 as i32);

    pub fn drag(hwnd: isize, path: &str) -> Result<bool, String> {
        unsafe {
            let hr = OleInitialize(None);
            let owns_ole = hr == S_OK || hr == windows::Win32::Foundation::S_FALSE;
            let _guard = scopeguard(owns_ole);

            if hr == RPC_E_CHANGED_MODE {
                return Err("El hilo de UI no está en STA".into());
            }

            let item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(path), None).map_err(|e| format!("{e:?}"))?;
            let data: IDataObject = item.BindToHandler(None, &BHID_DataObject).map_err(|e| format!("{e:?}"))?;

            let mut effect = DROPEFFECT(0);
            let hr = SHDoDragDrop(Some(HWND(hwnd as _)), Some(&data), None, DROPEFFECT_COPY, &mut effect);
            Ok(hr == DRAGDROP_S_DROP && effect != DROPEFFECT(0))
        }
    }
}
```

El `scopeguard` es una función local que llama a `OleUninitialize` al salir solo si esta
llamada fue la que inicializó OLE, para no desmontar el que ya tenía el bucle de eventos.

- [ ] **Step 2: Registrar el comando en `lib.rs`**

```rust
#[tauri::command]
async fn start_file_drag(app: tauri::AppHandle, path: String) -> Result<bool, String> {
    use tauri::Manager;
    if !std::path::Path::new(&path).is_file() {
        return Err("El archivo ya no existe".into());
    }
    let hwnd = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as isize)
        .unwrap_or(0);
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(dragdrop::drag(hwnd, &path));
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}
```

Añadir `mod dragdrop;` arriba y `start_file_drag` al `invoke_handler`.

- [ ] **Step 3: Compilar**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: sin errores. Si el compilador reclama features del crate `windows`, añadirlas a
`Cargo.toml` (no añadir crates nuevos).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/dragdrop.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "Add native OLE file drag so clips can be dropped into other apps"
```

---

### Task 3: Preparación del archivo a compartir

**Files:**
- Create: `src-tauri/src/share.rs`
- Modify: `src-tauri/src/lib.rs` (módulo, comandos, purga al arrancar)

**Interfaces:**
- Consumes: `editor::export_clip(..., bitrate, max_height, ...)` de la Task 1
- Produces: comandos `share_prepare(path, edit, target_bytes, watermark) -> Result<String, String>` y `share_cleanup()`

- [ ] **Step 1: Cálculo del objetivo**

```rust
pub struct Target { pub bitrate: u32, pub max_height: Option<u32> }

// Margen del 3% para las cabeceras del contenedor y el moov, que no entran en el bitrate.
pub fn plan(target_bytes: u64, duration_s: f64, audio_bps: u32, src_h: u32, src_fps: u32, src_w: u32) -> Target {
    let usable = (target_bytes as f64 * 8.0 * 0.97) - (audio_bps as f64 * duration_s);
    let bps = (usable / duration_s.max(0.1)).max(300_000.0) as u32;
    // Por debajo del umbral de calidad de la resolución actual la imagen se rompe en bloques:
    // bajar un peldaño de la escalera da mucha mejor imagen al mismo peso.
    let mut h = src_h;
    for step in [1080u32, 720, 480] {
        let threshold = src_w as u64 * h as u64 * src_fps.max(1) as u64 / 60;
        if (bps as u64) >= threshold || h <= 480 { break; }
        if step < h { h = step; }
    }
    Target { bitrate: bps, max_height: (h < src_h).then_some(h) }
}
```

- [ ] **Step 2: Camino rápido y caché**

`share_prepare` devuelve `path` sin tocar nada cuando la edición es identidad, no hay
`target_bytes` y no hay marca de agua. En cualquier otro caso, el destino es
`app_data/share/<hash>.mp4` donde el hash cubre `(path, mtime, edit serializado, target_bytes,
watermark)`; si ese archivo ya existe con tamaño > 0, se devuelve sin recodificar.

- [ ] **Step 3: Purga de temporales**

`share_cleanup()` borra de `app_data/share` todo lo que tenga más de 24 h. Se llama en el
`setup` de Tauri dentro de un `spawn_blocking`, para no retrasar el arranque.

- [ ] **Step 4: Emitir progreso**

`share_prepare` emite `share-progress` (no `export-progress`, que ya escucha el editor y
colisionaría si ambos están abiertos).

- [ ] **Step 5: Compilar y commit**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/share.rs src-tauri/src/lib.rs
git commit -m "Add share file preparation with size presets and temp cache"
```

---

### Task 4: Popup de compartir

**Files:**
- Create: `src/lib/share.svelte.ts`
- Create: `src/lib/components/ShareDialog.svelte`
- Modify: `src/lib/components/ClipCard.svelte:202`
- Modify: `src/lib/components/Editor.svelte` (botón junto a Exportar)
- Modify: `src/lib/i18n.svelte.ts`
- Modify: `src/routes/+layout.svelte` (montar el diálogo una sola vez)

**Interfaces:**
- Consumes: `start_file_drag`, `share_prepare`, evento `share-progress`
- Produces: `openShare(clip, edit?, watermark?)`, `closeShare()`, `shareState`

- [ ] **Step 1: Estado**

```ts
export const shareState = $state<{
  open: boolean; clip: Clip | null; edit: ClipEdit | null;
  preset: number | null; preparing: boolean; progress: number;
  ready: string | null; error: string | null; dragging: boolean;
}>({ ... });
```

- [ ] **Step 2: Diálogo**

Backdrop + tarjeta centrada, reutilizando el lenguaje visual de `.export-card` del editor.
Fila de presets (Original / 10 / 50 / 100 MB) con los que superan el tamaño del clip
deshabilitados. Zona de arrastre con borde discontinuo que pasa a sólido al hover, `poster`
estático y `<video muted loop>` montado solo al hover (mismo patrón que `ClipCard`), y badge
de duración arriba a la derecha.

- [ ] **Step 3: Gesto de arrastre**

`pointerdown` guarda el origen; en `pointermove`, si el desplazamiento supera 4 px, se invoca
`start_file_drag`. El elemento lleva `draggable="false"` para que el arrastre propio de
WebView2 no compita con el de OLE.

- [ ] **Step 4: Puntos de entrada**

`ClipCard.svelte:202` sustituye el `stopPropagation` por `openShare(clip)`. El editor añade un
botón Compartir junto a Exportar que pasa la edición actual y el estado de la marca de agua.

- [ ] **Step 5: i18n**

Claves `share.*` en español e inglés: título, etiqueta de tamaño, `Original`, textos de la
zona de arrastre, preparando, errores y el tooltip del preset deshabilitado.

- [ ] **Step 6: Verificar**

`pnpm check` sin errores. `pnpm tauri dev`: arrastrar a Discord, al Explorador y al escritorio;
comprobar el camino rápido (Original no genera temporal), un preset (genera y cabe), el preset
deshabilitado y el arrastre desde el editor con cortes activos.

- [ ] **Step 7: Commit**

```bash
git add src/lib/share.svelte.ts src/lib/components/ShareDialog.svelte src/lib/components/ClipCard.svelte src/lib/components/Editor.svelte src/lib/i18n.svelte.ts src/routes/+layout.svelte
git commit -m "Add share dialog with drag-to-app and file size presets"
```
