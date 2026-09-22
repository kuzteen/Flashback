# Editor: formato vertical 9:16 — Plan de implementación

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Exportar y compartir en vertical 1080×1920 con dos modos (recorte con encuadre por bloque, o encajado sobre fondo desenfocado), elegidos en un panel plegable del editor con previsualización en el visor.

**Architecture:** El formato viaja en `ClipEdit.format` y el encuadre en `Segment.crop_x` (opcionales, compatibles). En el export, el formato vertical fuerza el camino que recodifica y añade una etapa `Reframer` entre el decodificador y la marca de agua: con **Direct2D sobre el mismo device D3D11 del decodificador** (como ya hacen `watermark.rs` y `overlay.rs`) copia el fotograma a una textura propia y lo dibuja recortado o encajado sobre una textura de salida 1080×1920 que entrega un `IMFVideoSampleAllocatorEx` (reciclaje de texturas). El frontend añade `FormatPanel` y previsualiza en `Viewer` sin coste de export.

**Tech Stack:** Rust + `windows` 0.61 (Direct2D, D3D11, Media Foundation), Svelte 5, Vitest / `cargo test`.

**Spec:** `docs/superpowers/specs/2026-09-22-editor-redesign-design.md` (secciones 1, 3, 4). Consume los planes 1 y 2.

## Cambios respecto a la spec (decididos al estudiar el código)

1. **Direct2D en vez de procesador de vídeo + shader.** El export ya compone la marca de agua con Direct2D sobre el device del decodificador, y `overlay.rs` ya usa el desenfoque gaussiano de Direct2D con este mismo crate. Una sola API, cero copias a CPU, y un desenfoque real en vez de reducir y ampliar.
2. **Vertical solo con GPU.** La captura (WGC) ya exige D3D11 por hardware: un equipo sin GPU no graba clips. Sin GPU el export vertical devuelve un error claro en vez de mantener un camino por CPU que no se usaría.

## Global Constraints

- Sin dependencias nuevas; las features de `windows` necesarias ya están en `Cargo.toml`.
- Salida vertical: **1080×1920**; con un preset de compartir, `max_height` se interpreta como el lado corto (720 → 720×1280), pares.
- `crop_x` por defecto **0,5**; ventana de recorte = altura completa del origen × (altura·9/16), centrada en `crop_x·ancho` y limitada a los bordes.
- Encajado: fondo = recorte centrado escalado a pantalla completa, desenfoque gaussiano (desviación 3 % del ancho de salida), velo negro al 35 %; delante, el fotograma entero escalado para caber, centrado.
- Bitrate vertical sin preset = bitrate del origen × (píxeles salida / píxeles origen), limitado a [4, 40] Mbps. Con preset manda el del preset.
- El formato vertical **nunca** se copia sin recodificar y **nunca** devuelve el original al compartir.
- Comentarios en español solo para el porqué; commits en inglés, autor único, sin `Co-Authored-By`.
- Tooltips con `data-tip`, nunca `title`.

---

## Estructura de archivos

| Archivo | Responsabilidad |
|---|---|
| `src-tauri/src/reframe.rs` (nuevo) | `OutputFormat`, `Fill`, geometría pura (`vertical_size`, `vertical_bitrate`, `crop_rect`, `fit_rect`) con tests; submódulo `win` con `Reframer` (Direct2D). |
| `src-tauri/src/lib.rs` | `mod reframe;` |
| `src-tauri/src/editor.rs` | `ClipEdit.format`, `Segment.crop_x`, `is_noop`, decisión de passthrough, bitrate vertical, integración del `Reframer` en `reencode_export`. |
| `src-tauri/src/share.rs` | `is_identity` falso con formato vertical; literales de test. |
| `src/lib/editor-state.svelte.ts` | Enviar `format` al exportar y compartir. |
| `src/lib/share.svelte.ts` | `ShareEdit.format`. |
| `src/lib/components/editor/ui.svelte.ts` | Estado plegado del panel (preferencia local). |
| `src/lib/components/editor/FormatPanel.svelte` (nuevo) | Panel plegable de formato. |
| `src/lib/components/editor/Editor.svelte` | Fila central visor + panel. |
| `src/lib/components/editor/OutputBar.svelte` | Pasar `format` a compartir. |
| `src/lib/components/editor/Viewer.svelte` | Previsualización: marco de recorte arrastrable / composición encajada. |
| `src/lib/i18n.svelte.ts` | Textos. |
| `docs/superpowers/specs/2026-09-22-editor-redesign-design.md` | Reflejar los dos cambios de arriba. |

---

### Task 1: Formato de salida y geometría (Rust, puro)

**Files:**
- Create: `src-tauri/src/reframe.rs`
- Modify: `src-tauri/src/lib.rs` (lista de `mod`)
- Modify: `src-tauri/src/editor.rs` (`Segment`, `ClipEdit`, `is_noop`, `load_edit`, literales de test)
- Modify: `src-tauri/src/share.rs` (`is_identity`, literal de test)

**Interfaces:**
- Produces:
  - `reframe::Fill { Crop, Fit }` (serde minúsculas), `reframe::OutputFormat { Horizontal, Vertical { fill } }` (serde `tag = "kind"`, minúsculas; `Default = Horizontal`)
  - `reframe::Rect { x, y, w, h: f32 }`
  - `reframe::vertical_size(max_short: Option<u32>) -> (u32, u32)`
  - `reframe::vertical_bitrate(src_bps, src_w, src_h, out_w, out_h: u32) -> u32`
  - `reframe::crop_rect(src_w, src_h: u32, crop_x: f64) -> Rect`
  - `reframe::fit_rect(src_w, src_h, out_w, out_h: u32) -> Rect`
  - `editor::Segment.crop_x: Option<f64>`, `editor::ClipEdit.format: OutputFormat`

- [ ] **Step 1: Crear el módulo con tests que fallan**

Crear `src-tauri/src/reframe.rs`:

```rust
// Formato de salida del export y geometría del encuadre vertical. La geometría es pura y se prueba
// en cualquier plataforma; la composición en GPU vive en el submódulo de Windows.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fill {
    #[default]
    Crop,
    Fit,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Horizontal,
    Vertical { fill: Fill },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub fn vertical_size(_max_short: Option<u32>) -> (u32, u32) {
    todo!()
}

pub fn vertical_bitrate(_src_bps: u32, _src_w: u32, _src_h: u32, _out_w: u32, _out_h: u32) -> u32 {
    todo!()
}

pub fn crop_rect(_src_w: u32, _src_h: u32, _crop_x: f64) -> Rect {
    todo!()
}

pub fn fit_rect(_src_w: u32, _src_h: u32, _out_w: u32, _out_h: u32) -> Rect {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn vertical_is_1080_by_1920_and_a_preset_sets_the_short_side() {
        assert_eq!(vertical_size(None), (1080, 1920));
        assert_eq!(vertical_size(Some(720)), (720, 1280));
        assert_eq!(vertical_size(Some(480)), (480, 854));
        assert_eq!(vertical_size(Some(2160)), (1080, 1920));
        assert_eq!(vertical_size(Some(481)), (480, 854));
    }

    #[test]
    fn vertical_bitrate_follows_the_pixel_count_within_bounds() {
        assert_eq!(vertical_bitrate(20_000_000, 1920, 1080, 1080, 1920), 20_000_000);
        assert_eq!(vertical_bitrate(1_000_000, 1920, 1080, 1080, 1920), 4_000_000);
        assert_eq!(vertical_bitrate(100_000_000, 1920, 1080, 1080, 1920), 40_000_000);
    }

    #[test]
    fn the_crop_window_is_full_height_nine_by_sixteen_and_clamped() {
        assert_eq!(crop_rect(1920, 1080, 0.5), r(656.25, 0.0, 607.5, 1080.0));
        assert_eq!(crop_rect(1920, 1080, 0.0), r(0.0, 0.0, 607.5, 1080.0));
        assert_eq!(crop_rect(1920, 1080, 1.0), r(1312.5, 0.0, 607.5, 1080.0));
        assert_eq!(crop_rect(1920, 1080, 7.0), crop_rect(1920, 1080, 1.0));
    }

    #[test]
    fn an_already_vertical_source_is_used_whole() {
        assert_eq!(crop_rect(1080, 1920, 0.3), r(0.0, 0.0, 1080.0, 1920.0));
    }

    #[test]
    fn fit_scales_the_frame_to_the_width_and_centres_it() {
        assert_eq!(fit_rect(1920, 1080, 1080, 1920), r(0.0, 656.25, 1080.0, 607.5));
        assert_eq!(fit_rect(1080, 1920, 1080, 1920), r(0.0, 0.0, 1080.0, 1920.0));
    }

    #[test]
    fn formats_read_and_write_the_frontend_shape() {
        let v: OutputFormat = serde_json::from_str(r#"{"kind":"vertical","fill":"fit"}"#).unwrap();
        assert_eq!(v, OutputFormat::Vertical { fill: Fill::Fit });
        let h: OutputFormat = serde_json::from_str(r#"{"kind":"horizontal"}"#).unwrap();
        assert_eq!(h, OutputFormat::Horizontal);
        assert_eq!(
            serde_json::to_string(&OutputFormat::Vertical { fill: Fill::Crop }).unwrap(),
            r#"{"kind":"vertical","fill":"crop"}"#
        );
    }

    #[test]
    fn an_edit_saved_before_the_format_existed_reads_as_horizontal_and_centred() {
        let e: crate::editor::ClipEdit = serde_json::from_str(
            r#"{"segments":[{"start_ms":0,"end_ms":1000}],"mixer":{"sys_vol":1,"sys_muted":false,"mic_vol":1,"mic_muted":false}}"#,
        )
        .unwrap();
        assert_eq!(e.format, OutputFormat::Horizontal);
        assert_eq!(e.segments[0].crop_x, None);
    }
}
```

En `src-tauri/src/lib.rs`, tras `mod playlists;` añadir `mod reframe;`.

En `src-tauri/src/editor.rs`, dentro de `pub struct Segment`, tras el campo `disabled`:

```rust
    // Centro horizontal del marco 9:16 del formato vertical (0..1). Sin él, centrado.
    #[serde(default)]
    pub crop_x: Option<f64>,
```

Y en `pub struct ClipEdit`, tras `mixer`:

```rust
    #[serde(default)]
    pub format: crate::reframe::OutputFormat,
```

Añadir `format: Default::default(),` al `ClipEdit { … }` de `load_edit` y al de los tests de `editor.rs` y `share.rs`; añadir `crop_x: None,` a los `Segment { … }` de esos tests.

- [ ] **Step 2: Ejecutar para ver que fallan**

Run: `cd src-tauri && cargo test --lib reframe`
Expected: FAIL con `not yet implemented` en los tests de geometría; los dos de serde pasan.

- [ ] **Step 3: Implementar la geometría**

Sustituir las cuatro funciones `todo!()` de `reframe.rs`:

```rust
pub const VERTICAL_W: u32 = 1080;

// Un preset de compartir que baja la resolución fija el lado corto: así 720 da 720x1280, con el
// mismo número de píxeles que el 1280x720 horizontal que ya calcula el reparto de bitrate.
pub fn vertical_size(max_short: Option<u32>) -> (u32, u32) {
    let w = max_short.map_or(VERTICAL_W, |m| m.min(VERTICAL_W)) & !1;
    let h = ((w as u64 * 16 / 9) as u32 + 1) & !1;
    (w, h)
}

// El recorte vertical amplía una ventana del origen, así que el bitrate se escala por píxeles de
// salida y no se copia tal cual; los límites evitan un clip inservible o desproporcionado.
pub fn vertical_bitrate(src_bps: u32, src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> u32 {
    let src_px = (src_w as u64 * src_h as u64).max(1);
    let out_px = out_w as u64 * out_h as u64;
    (src_bps as u64 * out_px / src_px).clamp(4_000_000, 40_000_000) as u32
}

pub fn crop_rect(src_w: u32, src_h: u32, crop_x: f64) -> Rect {
    let (sw, sh) = (src_w as f32, src_h as f32);
    let w = (sh * 9.0 / 16.0).min(sw);
    let center = crop_x.clamp(0.0, 1.0) as f32 * sw;
    let x = (center - w / 2.0).clamp(0.0, sw - w);
    Rect { x, y: 0.0, w, h: sh }
}

pub fn fit_rect(src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> Rect {
    let (sw, sh, ow, oh) = (src_w as f32, src_h as f32, out_w as f32, out_h as f32);
    let s = (ow / sw).min(oh / sh);
    let (w, h) = (sw * s, sh * s);
    Rect { x: (ow - w) / 2.0, y: (oh - h) / 2.0, w, h }
}
```

- [ ] **Step 4: El formato vertical es una edición y nunca es la identidad**

En `editor.rs`, al principio de `fn is_noop`:

```rust
    if edit.format != crate::reframe::OutputFormat::Horizontal {
        return false;
    }
```

En `share.rs`, al principio de `pub fn is_identity`:

```rust
    if edit.format != crate::reframe::OutputFormat::Horizontal {
        return false;
    }
```

- [ ] **Step 5: Ejecutar y ver que pasa**

Run: `cd src-tauri && cargo test --lib`
Expected: PASS (todos, incluidos los 7 de `reframe`).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/reframe.rs src-tauri/src/lib.rs src-tauri/src/editor.rs src-tauri/src/share.rs
git commit -F - <<'EOF'
Export: output format and vertical framing geometry

An edit can now ask for a vertical 9:16 output, cropped per block or
fitted over a blurred background, and each segment carries where its
crop window sits. Both fields are optional when reading, so every edit
saved before reads as horizontal and centred.

The geometry is pure and tested: the full-height 9:16 window clamped to
the frame, the fit rectangle, the output size (1080x1920, or the short
side of a share preset) and a bitrate scaled by output pixels within
4-40 Mbps. A vertical edit is never a no-op and is never shared as the
untouched original.
EOF
```

---

### Task 2: Etapa Reframer en GPU e integración en el export

**Files:**
- Modify: `src-tauri/src/reframe.rs` (submódulo `win`)
- Modify: `src-tauri/src/editor.rs` (`do_export`, `reencode_export`)

**Interfaces:**
- Consumes: Task 1; `Gpu { device, manager }`, `negotiate_video`, `blend_watermark`, `create_sink` de `editor.rs`.
- Produces: `reframe::win::Reframer::new(device, manager, src_w, src_h, out_w, out_h, fps, fill) -> Result<Reframer>`, `.out_type() -> &IMFMediaType`, `.out_size() -> (u32, u32)`, `.process(&IMFSample, crop_x: f64) -> Result<IMFSample>`.

Nota: las firmas de `windows` 0.61 usadas aquí siguen las de `overlay.rs` (Direct2D) y `capture/win/mod.rs` (`MFCreateDXGISurfaceBuffer`). Si `cargo check` señala una diferencia de firma (p. ej. el genérico de `MFCreateVideoSampleAllocatorEx` o el tipo de `SetDirectXManager`), se ajusta la llamada sin cambiar el comportamiento.

- [ ] **Step 1: Escribir el Reframer**

Añadir al final de `src-tauri/src/reframe.rs`:

```rust
#[cfg(target_os = "windows")]
pub mod win {
    use super::{crop_rect, fit_rect, Fill, Rect};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use windows::core::{Interface, Result, IUnknown};
    use windows::Win32::Foundation::E_POINTER;
    use windows::Win32::Graphics::Direct2D::Common::{
        D2D1_ALPHA_MODE_IGNORE, D2D1_BORDER_MODE_HARD, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER,
        D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
    };
    use windows::Win32::Graphics::Direct2D::{
        D2D1CreateDevice, ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Effect, ID2D1SolidColorBrush,
        CLSID_D2D1GaussianBlur, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_NONE,
        D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
        D2D1_GAUSSIANBLUR_OPTIMIZATION_SPEED, D2D1_GAUSSIANBLUR_PROP_BORDER_MODE,
        D2D1_GAUSSIANBLUR_PROP_OPTIMIZATION, D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION,
        D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, D2D1_INTERPOLATION_MODE_LINEAR,
        D2D1_PROPERTY_TYPE_ENUM, D2D1_PROPERTY_TYPE_FLOAT,
    };
    use windows::Win32::Graphics::Direct3D11::{
        ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET,
        D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGISurface};
    use windows::Win32::Media::MediaFoundation::{
        IMF2DBuffer, IMFDXGIBuffer, IMFDXGIDeviceManager, IMFMediaType, IMFSample,
        IMFVideoSampleAllocatorEx, MFCreateAttributes, MFCreateMediaType,
        MFCreateVideoSampleAllocatorEx, MFMediaType_Video, MFVideoFormat_ARGB32,
        MFVideoInterlace_Progressive, MF_E_SAMPLEALLOCATOR_EMPTY, MF_MT_ALL_SAMPLES_INDEPENDENT,
        MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
        MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_SA_D3D11_BINDFLAGS, MF_SA_D3D11_USAGE,
    };

    fn d2d(r: Rect) -> D2D_RECT_F {
        D2D_RECT_F { left: r.x, top: r.y, right: r.x + r.w, bottom: r.y + r.h }
    }

    fn bgra(options: windows::Win32::Graphics::Direct2D::D2D1_BITMAP_OPTIONS) -> D2D1_BITMAP_PROPERTIES1 {
        D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: options,
            ..Default::default()
        }
    }

    // Recorta o encaja cada fotograma decodificado en una textura vertical, todo en la GPU y sobre
    // el mismo device que el decodificador (la textura del fotograma no es accesible desde otro).
    // El fotograma se copia antes a una textura propia: el decodificador entrega subtexturas de un
    // array y Direct2D solo dibuja desde la subtextura 0.
    pub struct Reframer {
        ctx: ID2D1DeviceContext,
        d3d: ID3D11DeviceContext,
        src_tex: ID3D11Texture2D,
        src_bmp: ID2D1Bitmap1,
        bg_bmp: Option<ID2D1Bitmap1>,
        blur: Option<ID2D1Effect>,
        dim: ID2D1SolidColorBrush,
        allocator: IMFVideoSampleAllocatorEx,
        out_type: IMFMediaType,
        // Un bitmap destino por textura del asignador: crearlos por fotograma costaba sin motivo.
        targets: RefCell<HashMap<usize, ID2D1Bitmap1>>,
        fill: Fill,
        src_w: u32,
        src_h: u32,
        out_w: u32,
        out_h: u32,
    }

    impl Reframer {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            device: &ID3D11Device,
            manager: &IMFDXGIDeviceManager,
            src_w: u32,
            src_h: u32,
            out_w: u32,
            out_h: u32,
            fps: u32,
            fill: Fill,
        ) -> Result<Self> {
            let dxgi: IDXGIDevice = device.cast()?;
            let d2d_device = unsafe { D2D1CreateDevice(&dxgi, None)? };
            let ctx = unsafe { d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)? };
            let d3d = unsafe { device.GetImmediateContext()? };

            let desc = D3D11_TEXTURE2D_DESC {
                Width: src_w,
                Height: src_h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut t: Option<ID3D11Texture2D> = None;
            unsafe { device.CreateTexture2D(&desc, None, Some(&mut t))? };
            let src_tex = t.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
            let surface: IDXGISurface = src_tex.cast()?;
            let src_bmp = unsafe { ctx.CreateBitmapFromDxgiSurface(&surface, Some(&bgra(D2D1_BITMAP_OPTIONS_NONE)))? };

            let (bg_bmp, blur) = if fill == Fill::Fit {
                let bg = unsafe {
                    ctx.CreateBitmap(
                        D2D_SIZE_U { width: out_w, height: out_h },
                        None,
                        0,
                        &bgra(D2D1_BITMAP_OPTIONS_TARGET),
                    )?
                };
                let blur = unsafe { ctx.CreateEffect(&CLSID_D2D1GaussianBlur)? };
                let deviation = out_w as f32 * 0.03;
                unsafe {
                    blur.SetValue(
                        D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,
                        D2D1_PROPERTY_TYPE_FLOAT,
                        &deviation.to_le_bytes(),
                    )?;
                    blur.SetValue(
                        D2D1_GAUSSIANBLUR_PROP_OPTIMIZATION.0 as u32,
                        D2D1_PROPERTY_TYPE_ENUM,
                        &(D2D1_GAUSSIANBLUR_OPTIMIZATION_SPEED.0 as u32).to_le_bytes(),
                    )?;
                    // HARD: sin él el borde del fondo se aclara hacia transparente.
                    blur.SetValue(
                        D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32,
                        D2D1_PROPERTY_TYPE_ENUM,
                        &(D2D1_BORDER_MODE_HARD.0 as u32).to_le_bytes(),
                    )?;
                }
                (Some(bg), Some(blur))
            } else {
                (None, None)
            };
            let dim = unsafe {
                ctx.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.35 }, None)?
            };

            let out_type = unsafe { MFCreateMediaType()? };
            unsafe {
                out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32)?;
                out_type.SetUINT64(&MF_MT_FRAME_SIZE, ((out_w as u64) << 32) | out_h as u64)?;
                out_type.SetUINT64(&MF_MT_FRAME_RATE, ((fps.max(1) as u64) << 32) | 1)?;
                out_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
                out_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                out_type.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
            }

            // El encoder retiene muestras de forma asíncrona: el asignador recicla una textura solo
            // cuando la suelta, en vez de reservar una por fotograma o pisar una que aún se lee.
            let allocator: IMFVideoSampleAllocatorEx = unsafe { MFCreateVideoSampleAllocatorEx()? };
            unsafe {
                allocator.SetDirectXManager(&manager.cast::<IUnknown>()?)?;
                let mut attrs = None;
                MFCreateAttributes(&mut attrs, 2)?;
                let attrs = attrs.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                attrs.SetUINT32(
                    &MF_SA_D3D11_BINDFLAGS,
                    (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                )?;
                attrs.SetUINT32(&MF_SA_D3D11_USAGE, D3D11_USAGE_DEFAULT.0 as u32)?;
                allocator.InitializeSampleAllocatorEx(4, 16, &attrs, &out_type)?;
            }

            Ok(Self {
                ctx,
                d3d,
                src_tex,
                src_bmp,
                bg_bmp,
                blur,
                dim,
                allocator,
                out_type,
                targets: RefCell::new(HashMap::new()),
                fill,
                src_w,
                src_h,
                out_w,
                out_h,
            })
        }

        pub fn out_type(&self) -> &IMFMediaType {
            &self.out_type
        }

        pub fn out_size(&self) -> (u32, u32) {
            (self.out_w, self.out_h)
        }

        pub fn process(&self, input: &IMFSample, crop_x: f64) -> Result<IMFSample> {
            self.load_input(input)?;
            let out = self.next_sample()?;
            let target = self.target_for(&out)?;
            let full = D2D_RECT_F { left: 0.0, top: 0.0, right: self.out_w as f32, bottom: self.out_h as f32 };
            unsafe {
                match (self.fill, &self.bg_bmp, &self.blur) {
                    (Fill::Fit, Some(bg), Some(blur)) => {
                        // Fondo: el recorte centrado a pantalla completa, desenfocado y velado para
                        // que el vídeo de delante destaque.
                        let cover = d2d(crop_rect(self.src_w, self.src_h, 0.5));
                        self.ctx.SetTarget(bg);
                        self.ctx.BeginDraw();
                        self.ctx.DrawBitmap(&self.src_bmp, Some(&full), 1.0, D2D1_INTERPOLATION_MODE_LINEAR, Some(&cover), None);
                        self.ctx.EndDraw(None, None)?;

                        self.ctx.SetTarget(&target);
                        self.ctx.BeginDraw();
                        blur.SetInput(0, bg, true);
                        let img = blur.GetOutput()?;
                        self.ctx.DrawImage(&img, None, None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
                        self.ctx.FillRectangle(&full, &self.dim);
                        let fg = d2d(fit_rect(self.src_w, self.src_h, self.out_w, self.out_h));
                        self.ctx.DrawBitmap(&self.src_bmp, Some(&fg), 1.0, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, None, None);
                        self.ctx.EndDraw(None, None)?;
                    }
                    _ => {
                        let src = d2d(crop_rect(self.src_w, self.src_h, crop_x));
                        self.ctx.SetTarget(&target);
                        self.ctx.BeginDraw();
                        self.ctx.DrawBitmap(&self.src_bmp, Some(&full), 1.0, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, Some(&src), None);
                        self.ctx.EndDraw(None, None)?;
                    }
                }
                self.ctx.SetTarget(None);
            }
            Ok(out)
        }

        // Copia GPU→GPU del fotograma a la textura de origen. Si el lector lo dio en memoria de
        // sistema (negociación sin texturas), se sube; nunca se baja nada a la CPU.
        fn load_input(&self, input: &IMFSample) -> Result<()> {
            let buf = unsafe { input.GetBufferByIndex(0)? };
            if let Ok(dxgi) = buf.cast::<IMFDXGIBuffer>() {
                let mut tex: Option<ID3D11Texture2D> = None;
                unsafe {
                    dxgi.GetResource(
                        &ID3D11Texture2D::IID,
                        &mut tex as *mut Option<ID3D11Texture2D> as *mut *mut std::ffi::c_void,
                    )?;
                }
                let tex = tex.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                unsafe { tex.GetDesc(&mut desc) };
                if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
                    return Err(windows::core::Error::new(E_POINTER, "formato de fotograma no soportado"));
                }
                let sub = unsafe { dxgi.GetSubresourceIndex()? };
                let region = D3D11_BOX { left: 0, top: 0, front: 0, right: self.src_w, bottom: self.src_h, back: 1 };
                unsafe { self.d3d.CopySubresourceRegion(&self.src_tex, 0, 0, 0, 0, &tex, sub, Some(&region)) };
                return Ok(());
            }
            let b2: IMF2DBuffer = buf.cast()?;
            let mut scan0: *mut u8 = std::ptr::null_mut();
            let mut pitch: i32 = 0;
            unsafe { b2.Lock2D(&mut scan0, &mut pitch)? };
            if pitch > 0 && !scan0.is_null() {
                unsafe {
                    self.d3d.UpdateSubresource(&self.src_tex, 0, None, scan0 as *const std::ffi::c_void, pitch as u32, 0)
                };
            }
            unsafe { b2.Unlock2D()? };
            Ok(())
        }

        // Si todas las texturas siguen en el encoder, se espera a que suelte una; el límite evita
        // colgar el export si el encoder se atasca.
        fn next_sample(&self) -> Result<IMFSample> {
            for _ in 0..2000 {
                match unsafe { self.allocator.AllocateSample() } {
                    Ok(s) => return Ok(s),
                    Err(e) if e.code() == MF_E_SAMPLEALLOCATOR_EMPTY => {
                        std::thread::sleep(std::time::Duration::from_millis(2))
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(windows::core::Error::from(MF_E_SAMPLEALLOCATOR_EMPTY))
        }

        fn target_for(&self, sample: &IMFSample) -> Result<ID2D1Bitmap1> {
            let buf = unsafe { sample.GetBufferByIndex(0)? };
            let dxgi: IMFDXGIBuffer = buf.cast()?;
            let mut tex: Option<ID3D11Texture2D> = None;
            unsafe {
                dxgi.GetResource(
                    &ID3D11Texture2D::IID,
                    &mut tex as *mut Option<ID3D11Texture2D> as *mut *mut std::ffi::c_void,
                )?;
            }
            let tex = tex.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
            let key = tex.as_raw() as usize;
            if let Some(b) = self.targets.borrow().get(&key) {
                return Ok(b.clone());
            }
            let surface: IDXGISurface = tex.cast()?;
            let bmp = unsafe {
                self.ctx.CreateBitmapFromDxgiSurface(
                    &surface,
                    Some(&bgra(D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW)),
                )?
            };
            self.targets.borrow_mut().insert(key, bmp.clone());
            Ok(bmp)
        }
    }
}
```

- [ ] **Step 2: Comprobar que compila**

Run: `cd src-tauri && cargo check`
Expected: sin errores (ajustar firmas si el crate difiere, ver nota de la tarea). `reframe::win` aún no se usa: el aviso de código muerto es esperable hasta el Step 3.

- [ ] **Step 3: Integrar en `do_export`**

En `src-tauri/src/editor.rs`, dentro de `fn do_export`:

Tras `if let Some(bps) = bitrate { meta.bitrate = bps.max(100_000); }` añadir:

```rust
        // Sin preset, el bitrate vertical se escala por píxeles de salida: el recorte amplía una
        // ventana del origen y copiar el bitrate de 1920x1080 no siempre encaja.
        let vertical = matches!(edit.format, crate::reframe::OutputFormat::Vertical { .. });
        if vertical && bitrate.is_none() {
            let (vw, vh) = crate::reframe::vertical_size(max_height);
            meta.bitrate = crate::reframe::vertical_bitrate(meta.bitrate, meta.width, meta.height, vw, vh);
        }
```

En la condición `let can_pass = watermark.is_none() && …`, añadir `&& !vertical`.

Tras `let gpu = create_gpu();` añadir:

```rust
        // La captura ya exige D3D11 por hardware, así que no hay camino por CPU para el vertical.
        if vertical && gpu.is_none() {
            return Err("El formato vertical necesita una GPU compatible con DirectX 11".into());
        }
```

- [ ] **Step 4: Integrar en `reencode_export`**

En `src-tauri/src/editor.rs`, dentro de `fn reencode_export`:

1. Sustituir el bloque `let (req_w, req_h) = match max_height { … };` por:

```rust
        let vertical = match edit.format {
            crate::reframe::OutputFormat::Vertical { fill } => Some(fill),
            crate::reframe::OutputFormat::Horizontal => None,
        };
        // En vertical el origen se decodifica a tamaño nativo: el recorte amplía una ventana y
        // reescalar antes solo perdería detalle. El preset se aplica al tamaño de salida.
        let (req_w, req_h) = match max_height {
            Some(mh) if mh < meta.height && vertical.is_none() => {
                let w = (meta.width as u64 * mh as u64 / meta.height.max(1) as u64) as u32;
                ((w + 1) & !1, (mh + 1) & !1)
            }
            _ => (meta.width, meta.height),
        };
```

2. En la llamada a `negotiate_video`, sustituir el argumento `watermark.is_some(),` por `watermark.is_some() || vertical.is_some(),` (el Reframer compone en BGRA, como la marca de agua).

3. Tras `let out_h = (out_size & 0xFFFF_FFFF) as u32;` añadir:

```rust
        let reframer = match (vertical, gpu) {
            (Some(fill), Some(g)) => {
                let (vw, vh) = crate::reframe::vertical_size(max_height);
                Some(
                    crate::reframe::win::Reframer::new(&g.device, &g.manager, out_w, out_h, vw, vh, meta.fps, fill)
                        .map_err(mf)?,
                )
            }
            _ => None,
        };
        // Tamaño de lo que llega al encoder: el vertical si hay Reframer, el decodificado si no.
        let (enc_w, enc_h) = reframer.as_ref().map_or((out_w, out_h), |r| r.out_size());
        let gpu_frames = on_gpu || reframer.is_some();
```

4. En el rasterizado de la marca de agua, sustituir `crate::watermark::Logo::rasterize(out_w, out_h, …)` por `crate::watermark::Logo::rasterize(enc_w, enc_h, …)`, y en `let gpu_logo = match (&logo, gpu) { (Some(l), Some(g)) if on_gpu => …` sustituir `if on_gpu` por `if gpu_frames`.

5. En `create_sink(dst, gpu.filter(|_| on_gpu).map(|g| &g.manager), false)` sustituir `on_gpu` por `gpu_frames`.

6. En `v_out.SetUINT64(&MF_MT_FRAME_SIZE, pack2(out_w, out_h))` sustituir por `pack2(enc_w, enc_h)`.

7. Sustituir `unsafe { sink.SetInputMediaType(v_stream, &v_in, None).map_err(mf)? };` por:

```rust
        let enc_in = reframer.as_ref().map_or(&v_in, |r| r.out_type());
        unsafe { sink.SetInputMediaType(v_stream, enc_in, None).map_err(mf)? };
```

8. En el bucle de fotogramas, justo antes de `if let Some(logo) = &logo {`, añadir:

```rust
                let sample = match &reframer {
                    Some(r) => r.process(&sample, seg.crop_x.unwrap_or(0.5)).map_err(mf)?,
                    None => sample,
                };
```

y en `blend_watermark(&sample, logo, gpu_logo.as_ref(), out_w, out_h);` sustituir `out_w, out_h` por `enc_w, enc_h`.

- [ ] **Step 5: Comprobar**

Run: `cd src-tauri && cargo check 2>&1 | grep -E "^(error|warning)"`
Expected: sin errores ni avisos.

Run: `cd src-tauri && cargo test --lib`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/reframe.rs src-tauri/src/editor.rs
git commit -F - <<'EOF'
Export: reframe to vertical on the GPU

A vertical edit always re-encodes, and a new stage between the decoder
and the watermark draws each frame into a 1080x1920 texture with
Direct2D on the decoder's own D3D11 device, the same way the watermark
and the minimised-game card already compose: cropped to the block's
9:16 window, or fitted whole over a blurred, dimmed copy of itself.

The decoded frame is copied GPU to GPU into a texture of its own first,
since the decoder hands out array slices and Direct2D only draws from
slice 0. Output textures come from a sample allocator that only recycles
one once the encoder lets it go, instead of allocating per frame or
overwriting a frame still being read. The source is decoded at native
size (a share preset sizes the output instead), the watermark lands on
the vertical canvas, and the bitrate follows the output pixel count.

Without a hardware D3D11 device the export stops with a clear error:
capture already requires one, so a CPU path would never run.
EOF
```

---

### Task 3: Formato en el editor (datos, panel y textos)

**Files:**
- Modify: `src/lib/editor-state.svelte.ts` (`exportClip`, `shareEdit`)
- Modify: `src/lib/share.svelte.ts` (`ShareEdit`)
- Modify: `src/lib/components/editor/OutputBar.svelte` (`openShare`)
- Modify: `src/lib/components/editor/ui.svelte.ts` (panel plegado)
- Create: `src/lib/components/editor/FormatPanel.svelte`
- Modify: `src/lib/components/editor/Editor.svelte` (fila central)
- Modify: `src/lib/i18n.svelte.ts`

**Interfaces:**
- Consumes: `OutputFormat`, `DEFAULT_FORMAT` (plan 1), `commit`.
- Produces: `ui.formatOpen`, `ui.toggleFormat()`; `<FormatPanel />`.

- [ ] **Step 1: Enviar el formato al exportar y compartir**

En `src/lib/editor-state.svelte.ts`:

- En el import de `./edit-model` añadir `DEFAULT_FORMAT` y `type OutputFormat`.
- En `exportClip`, sustituir `edit: { segments: s.segments, mixer: s.mixer }` por `edit: { segments: s.segments, mixer: s.mixer, format: s.format ?? DEFAULT_FORMAT }`.
- Sustituir la firma y el `return` de `shareEdit` por:

```ts
export function shareEdit(): {
  segments: SavedSegment[];
  mixer: MixerState;
  format: OutputFormat;
  keptSec: number;
} | null {
  const snap = snapshot();
  const s = toSaved(snap, true);
  if (s.segments.length === 0) return null;
  return { segments: s.segments, mixer: snap.mixer, format: snap.format, keptSec: keptMs(snap.segments) / 1000 };
}
```

En `src/lib/share.svelte.ts`: añadir `import type { OutputFormat } from './edit-model';` y cambiar `export type ShareEdit = { segments: SavedSegment[]; mixer: MixerState };` por `export type ShareEdit = { segments: SavedSegment[]; mixer: MixerState; format?: OutputFormat };`.

En `src/lib/components/editor/OutputBar.svelte`, sustituir `openShare(clip, { segments: s.segments, mixer: s.mixer }, watermark, s.keptSec);` por `openShare(clip, { segments: s.segments, mixer: s.mixer, format: s.format }, watermark, s.keptSec);`.

- [ ] **Step 2: Estado plegado del panel**

En `src/lib/components/editor/ui.svelte.ts`, dentro de la clase, tras `blockMenu = $state<…>(null);`:

```ts
  // Preferencia de quien edita: plegado por defecto y recordado entre sesiones.
  formatOpen = $state(readFormatOpen());

  toggleFormat() {
    this.formatOpen = !this.formatOpen;
    try {
      localStorage.setItem(FORMAT_KEY, this.formatOpen ? '1' : '0');
    } catch {}
  }
```

y encima de `class EditorUi {`:

```ts
const FORMAT_KEY = 'flashback.editor.formatPanel';

function readFormatOpen(): boolean {
  try {
    return localStorage.getItem(FORMAT_KEY) === '1';
  } catch {
    return false;
  }
}
```

(No se toca en `reset()`: es una preferencia, no estado del clip.)

- [ ] **Step 3: Textos**

En `src/lib/i18n.svelte.ts`, tras `  'key.del': 'Del',` añadir:

```ts
  'ed.format': 'Format',
  'ed.fmtHorizontal': 'Horizontal',
  'ed.fmtHorizontalHint': 'Original framing',
  'ed.fmtVertical': 'Vertical 9:16',
  'ed.fmtVerticalHint': 'TikTok, Shorts, Reels',
  'ed.fill': 'Fill',
  'ed.fillCrop': 'Crop',
  'ed.fillFit': 'Fit',
  'ed.cropHint': 'Drag the frame over the video. Each block keeps its own framing.',
  'ed.fitHint': 'The whole video over a blurred background.',
  'ed.cropFrame': 'Vertical frame',
```

y tras `  'key.del': 'Supr',`:

```ts
  'ed.format': 'Formato',
  'ed.fmtHorizontal': 'Horizontal',
  'ed.fmtHorizontalHint': 'Encuadre original',
  'ed.fmtVertical': 'Vertical 9:16',
  'ed.fmtVerticalHint': 'TikTok, Shorts, Reels',
  'ed.fill': 'Relleno',
  'ed.fillCrop': 'Recorte',
  'ed.fillFit': 'Encajado',
  'ed.cropHint': 'Arrastra el marco sobre el vídeo. Cada bloque guarda su propio encuadre.',
  'ed.fitHint': 'El vídeo entero sobre un fondo desenfocado.',
  'ed.cropFrame': 'Marco vertical',
```

- [ ] **Step 4: Crear FormatPanel**

Crear `src/lib/components/editor/FormatPanel.svelte`:

```svelte
<script lang="ts">
  import Icon from '../Icon.svelte';
  import { commit, editorState } from '$lib/editor-state.svelte';
  import type { OutputFormat } from '$lib/edit-model';
  import { t } from '$lib/i18n.svelte';
  import { ui } from './ui.svelte';

  const format = $derived(editorState.edit.format);

  // Cada cambio de formato es un paso de deshacer; elegir el que ya está no ensucia el historial.
  function set(next: OutputFormat) {
    if (JSON.stringify(next) === JSON.stringify(format)) return;
    commit({ ...editorState.edit, format: next });
  }
</script>

<aside class="panel" class:open={ui.formatOpen}>
  <button
    class="toggle"
    class:on={ui.formatOpen}
    aria-label={t('ed.format')}
    aria-expanded={ui.formatOpen}
    data-tip={t('ed.format')}
    data-tip-pos="below"
    data-tip-align="end"
    onclick={() => ui.toggleFormat()}
  >
    <Icon name="crop" size={18} />
  </button>

  {#if ui.formatOpen}
    <div class="body">
      <h3 class="title">{t('ed.format')}</h3>
      <button class="opt" class:on={format.kind === 'horizontal'} onclick={() => set({ kind: 'horizontal' })}>
        <span class="shape h"></span>
        <span class="txt">
          <span class="name">{t('ed.fmtHorizontal')}</span>
          <span class="hint">{t('ed.fmtHorizontalHint')}</span>
        </span>
      </button>
      <button
        class="opt"
        class:on={format.kind === 'vertical'}
        onclick={() => set({ kind: 'vertical', fill: format.kind === 'vertical' ? format.fill : 'crop' })}
      >
        <span class="shape v"></span>
        <span class="txt">
          <span class="name">{t('ed.fmtVertical')}</span>
          <span class="hint">{t('ed.fmtVerticalHint')}</span>
        </span>
      </button>

      {#if format.kind === 'vertical'}
        <div class="seg" role="radiogroup" aria-label={t('ed.fill')}>
          <button
            role="radio"
            aria-checked={format.fill === 'crop'}
            class:on={format.fill === 'crop'}
            onclick={() => set({ kind: 'vertical', fill: 'crop' })}
          >
            {t('ed.fillCrop')}
          </button>
          <button
            role="radio"
            aria-checked={format.fill === 'fit'}
            class:on={format.fill === 'fit'}
            onclick={() => set({ kind: 'vertical', fill: 'fit' })}
          >
            {t('ed.fillFit')}
          </button>
        </div>
        <p class="note">{format.fill === 'crop' ? t('ed.cropHint') : t('ed.fitHint')}</p>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .panel {
    flex: none;
    width: 52px;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 14px;
    padding: 10px 8px;
    background: var(--bg-0);
    border-left: 1px solid var(--line);
    transition: width 0.18s ease;
    overflow: hidden;
  }
  .panel.open {
    width: 260px;
    padding: 10px 14px 14px;
  }
  .toggle {
    align-self: flex-end;
    width: 36px;
    height: 36px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-2);
    border-radius: var(--r-sm);
    transition: color 0.14s ease, background 0.14s ease;
  }
  .toggle:hover,
  .toggle.on {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 230px;
  }
  .title {
    margin-bottom: 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px;
    text-align: left;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    transition: border-color 0.14s ease, background 0.14s ease;
  }
  .opt:hover {
    background: var(--bg-2);
  }
  .opt.on {
    border-color: var(--text-3);
    background: var(--bg-2);
  }
  /* Miniatura de la proporción: se entiende antes que el texto. */
  .shape {
    flex: none;
    border: 2px solid var(--text-3);
    border-radius: 3px;
  }
  .opt.on .shape {
    border-color: var(--text-0);
  }
  .shape.h {
    width: 28px;
    height: 16px;
  }
  .shape.v {
    width: 14px;
    height: 24px;
    margin: 0 7px;
  }
  .txt {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .name {
    font-size: 13px;
    color: var(--text-0);
  }
  .hint {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .seg {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
    padding: 3px;
    margin-top: 4px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
  }
  .seg button {
    height: 30px;
    font-size: 12.5px;
    color: var(--text-2);
    border-radius: 4px;
    transition: color 0.14s ease, background 0.14s ease;
  }
  .seg button:hover {
    color: var(--text-0);
  }
  .seg button.on {
    color: var(--text-0);
    background: var(--bg-3);
  }
  .note {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-3);
  }
</style>
```

- [ ] **Step 5: Fila central en el armazón**

En `src/lib/components/editor/Editor.svelte`:

- Añadir `import FormatPanel from './FormatPanel.svelte';` junto a los demás imports de piezas.
- Sustituir `  <Viewer />` por:

```svelte
  <div class="middle">
    <Viewer />
    <FormatPanel />
  </div>
```

- Añadir en el `<style>`, tras la regla `.ed { … }`:

```css
  .middle {
    flex: 1;
    min-height: 0;
    display: flex;
  }
```

- [ ] **Step 6: Comprobar y commit**

Run: `pnpm check` → `0 ERRORS`. Run: `pnpm test` → PASS.

```bash
git add src/lib/editor-state.svelte.ts src/lib/share.svelte.ts src/lib/components/editor/OutputBar.svelte src/lib/components/editor/ui.svelte.ts src/lib/components/editor/FormatPanel.svelte src/lib/components/editor/Editor.svelte src/lib/i18n.svelte.ts
git commit -F - <<'EOF'
Editor: collapsible format panel for vertical export

A panel on the right of the viewer, collapsed to a single button by
default and remembered across sessions, picks between the original
horizontal framing and vertical 9:16, cropped or fitted. Each change is
one undo step, and the format now travels with export and share, so
sharing a vertical edit sends a vertical file.
EOF
```

---

### Task 4: Previsualización en el visor

**Files:**
- Modify: `src/lib/components/editor/Viewer.svelte`

**Interfaces:**
- Consumes: `outToSeg`, `setCrop` (plan 1), `beginGesture`, `preview`, `endGesture`, `commit` (plan 2), `playback.outPos`.

- [ ] **Step 1: Estado, medida y gestos**

En `Viewer.svelte`, añadir a los imports:

```ts
  import { beginGesture, commit, endGesture, preview } from '$lib/editor-state.svelte';
  import { outToSeg, setCrop } from '$lib/edit-model';
```

(y añadir `beginGesture, commit, endGesture, preview` al import existente de `$lib/editor-state.svelte` en lugar de duplicarlo).

Añadir al final del `<script>`:

```ts
  const format = $derived(editorState.edit.format);
  const fill = $derived(format.kind === 'vertical' ? format.fill : null);

  // Caja real del vídeo dentro del escenario: el marco de recorte se dibuja encima de ella.
  let vbox = $state({ x: 0, y: 0, w: 0, h: 0 });

  function measure() {
    if (!video) return;
    vbox = { x: video.offsetLeft, y: video.offsetTop, w: video.offsetWidth, h: video.offsetHeight };
  }

  $effect(() => {
    const v = video;
    if (!v) return;
    const ro = new ResizeObserver(measure);
    ro.observe(v);
    measure();
    return () => ro.disconnect();
  });

  // El encuadre que se ve y se arrastra es el del bloque bajo el cabezal.
  const segIdx = $derived(outToSeg(editorState.edit.segments, playback.outPos).index);
  const cropX = $derived(editorState.edit.segments[segIdx]?.cropX ?? 0.5);
  const frameW = $derived(Math.min(vbox.w, (vbox.h * 9) / 16));
  // Misma cuenta que reframe::crop_rect: centrado en crop_x y limitado a los bordes.
  const frameLeft = $derived(Math.max(0, Math.min(vbox.w - frameW, cropX * vbox.w - frameW / 2)));

  let cropDrag: { index: number; startX: number; startCenter: number } | null = null;

  function clampCenter(c: number): number {
    const half = vbox.w > 0 ? frameW / 2 / vbox.w : 0.5;
    return Math.max(half, Math.min(1 - half, c));
  }

  function onCropDown(e: PointerEvent) {
    if (e.button !== 0 || vbox.w <= 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    beginGesture();
    // Se parte del centro efectivo y no de crop_x: si el marco está contra un borde, arrastrar
    // hacia dentro responde al instante en vez de recorrer primero una zona muerta.
    cropDrag = { index: segIdx, startX: e.clientX, startCenter: (frameLeft + frameW / 2) / vbox.w };
  }

  function onCropMove(e: PointerEvent) {
    if (!cropDrag || vbox.w <= 0) return;
    const next = clampCenter(cropDrag.startCenter + (e.clientX - cropDrag.startX) / vbox.w);
    preview({ ...editorState.edit, segments: setCrop(editorState.edit.segments, cropDrag.index, next) });
  }

  function onCropUp() {
    if (!cropDrag) return;
    cropDrag = null;
    endGesture();
  }

  function onCropKey(e: KeyboardEvent) {
    const step = e.key === 'ArrowLeft' ? -0.02 : e.key === 'ArrowRight' ? 0.02 : 0;
    if (!step) return;
    e.preventDefault();
    e.stopPropagation();
    const center = (frameLeft + frameW / 2) / Math.max(1, vbox.w);
    commit({ ...editorState.edit, segments: setCrop(editorState.edit.segments, segIdx, clampCenter(center + step)) });
  }

  // Encajado: el fondo es un lienzo diminuto al que se copia el fotograma, escalado y desenfocado
  // por CSS. Sin segundo decodificador y todo en la GPU del navegador.
  let bgCanvas = $state<HTMLCanvasElement | null>(null);

  function drawBg() {
    const c = bgCanvas;
    const v = video;
    if (!c || !v || !v.videoWidth) return;
    const ctx = c.getContext('2d');
    if (!ctx) return;
    const sw = Math.min(v.videoWidth, (v.videoHeight * 9) / 16);
    ctx.drawImage(v, (v.videoWidth - sw) / 2, 0, sw, v.videoHeight, 0, 0, c.width, c.height);
  }

  $effect(() => {
    if (fill === 'fit' && bgCanvas) drawBg();
  });
```

En el `$effect` de `requestVideoFrameCallback`, dentro de `const cb = (…) => {`, tras `playback.shownMediaTime = meta.mediaTime;` añadir:

```ts
      if (bgCanvas) drawBg();
```

- [ ] **Step 2: Marcado**

Sustituir el bloque:

```svelte
    <div class="fit">
      <video
```

hasta el cierre `</div>` de `.fit` por:

```svelte
    <div class="fit">
      <div class="frame" class:vfit={fill === 'fit'}>
        {#if fill === 'fit'}
          <canvas bind:this={bgCanvas} class="bg" width="54" height="96"></canvas>
        {/if}
        <video
          bind:this={video}
          src={editorState.videoSrc}
          playsinline
          class:fs={ui.fs}
          class:nocursor={ui.fs && !ui.fsCtrlShow}
          onloadedmetadata={onLoaded}
          onended={() => playback.pause()}
          onclick={() => playback.toggle()}
        ><track kind="captions" /></video>
      </div>
      {#if fill === 'crop' && vbox.w > 0 && !ui.fs}
        <div class="crop-view" style:left="{vbox.x}px" style:top="{vbox.y}px" style:width="{vbox.w}px" style:height="{vbox.h}px">
          <div
            class="crop-frame"
            style:left="{frameLeft}px"
            style:width="{frameW}px"
            role="slider"
            tabindex="0"
            aria-label={t('ed.cropFrame')}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(cropX * 100)}
            onpointerdown={onCropDown}
            onpointermove={onCropMove}
            onpointerup={onCropUp}
            onpointercancel={onCropUp}
            onkeydown={onCropKey}
          ></div>
        </div>
      {/if}
    </div>
```

- [ ] **Step 3: Estilos**

Añadir al `<style>` de `Viewer.svelte`:

```css
  /* En horizontal y en recorte el envoltorio no existe para el layout y el vídeo se coloca como
     siempre; en encajado pasa a ser el lienzo 9:16 que simula el resultado. */
  .frame {
    display: contents;
  }
  .frame.vfit {
    position: relative;
    display: block;
    height: 100%;
    aspect-ratio: 9 / 16;
    max-width: 100%;
    overflow: hidden;
    border-radius: var(--r-md);
    background: #000;
    box-shadow: 0 24px 60px -28px rgba(0, 0, 0, 0.9);
  }
  .frame.vfit video:not(.fs) {
    position: absolute;
    left: 0;
    top: 50%;
    width: 100%;
    max-width: none;
    max-height: none;
    transform: translateY(-50%);
    border-radius: 0;
    box-shadow: none;
  }
  .bg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    filter: blur(14px) brightness(0.65);
    transform: scale(1.15);
  }
  .crop-view {
    position: absolute;
    overflow: hidden;
    border-radius: var(--r-md);
    pointer-events: none;
  }
  /* La sombra enorme atenúa todo lo que queda fuera del marco sin otro elemento. */
  .crop-frame {
    position: absolute;
    top: 0;
    bottom: 0;
    border: 2px solid var(--accent);
    border-radius: 4px;
    box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.6);
    cursor: grab;
    pointer-events: auto;
    touch-action: none;
    outline: none;
  }
  .crop-frame:active {
    cursor: grabbing;
  }
  .crop-frame:focus-visible {
    border-color: var(--accent-soft);
  }
```

- [ ] **Step 4: Comprobar y commit**

Run: `pnpm check` → `0 ERRORS`. Run: `pnpm build` → `✔ done`.

```bash
git add src/lib/components/editor/Viewer.svelte
git commit -F - <<'EOF'
Editor: preview the vertical format in the viewer

Crop mode dims everything outside a 9:16 frame over the video; the
frame belongs to the block under the playhead and drags sideways (or
moves with the arrow keys), each drag being one undo step. Its maths
match the export's crop window, so what is framed is what is exported.

Fit mode turns the stage into a 9:16 canvas: the video sits centred and
the background is a tiny canvas the painted frame is copied into, scaled
up and blurred by CSS, so the preview costs no second decoder.
EOF
```

---

### Task 5: Especificación, verificación y prueba manual

**Files:**
- Modify: `docs/superpowers/specs/2026-09-22-editor-redesign-design.md` (sección 3)

- [ ] **Step 1: Actualizar la spec**

En la sección `## 3. Backend: formato vertical`, sustituir los párrafos de **Recorte** (la frase "en una pasada del procesador de vídeo de D3D11"), **Encajado** (fondo por miniatura, composición por dos flujos o pixel shader) y **Sin GPU** por:

```markdown
### Composición (recorte y encajado)

Una etapa `Reframer` (`src-tauri/src/reframe.rs`) dibuja con **Direct2D sobre el device D3D11 del
decodificador**, igual que la marca de agua y el cartel de juego minimizado. El fotograma se copia
GPU→GPU a una textura propia (el decodificador entrega subtexturas de un array y Direct2D solo dibuja
desde la 0) y se dibuja sobre una textura 1080×1920 de un `IMFVideoSampleAllocatorEx`, que recicla
texturas cuando el encoder las suelta.

- **Recorte**: ventana 9:16 de altura completa centrada en `crop_x` del bloque, escalada a la salida.
- **Encajado**: fondo = recorte centrado a pantalla completa con el desenfoque gaussiano de Direct2D
  (desviación 3 % del ancho) y un velo al 35 %; delante, el fotograma entero escalado y centrado.

### Sin GPU

No hay camino por CPU: la captura ya exige D3D11 por hardware, así que un equipo sin GPU no tiene
clips propios que exportar. El export vertical devuelve un error claro.
```

- [ ] **Step 2: Verificación completa**

Run: `cd src-tauri && cargo test --lib` → PASS. Run: `cd src-tauri && cargo check 2>&1 | grep -E "^(error|warning)"` → nada. Run: `pnpm test` → PASS. Run: `pnpm check` → `0 ERRORS`. Run: `pnpm build` → `✔ done`.

- [ ] **Step 3: Prueba manual** (`pnpm tauri dev`; la hace el usuario)

- El panel de formato abre y cierra con su botón y recuerda el estado al reabrir el editor.
- Horizontal: exportar sigue copiando sin recodificar cuando solo hay cortes en keyframe (mensaje de consola "copia sin recodificar").
- Vertical + recorte: el marco aparece sobre el vídeo, se arrastra, y al pasar el cabezal a otro bloque (tras cortar) muestra el encuadre de ese bloque; Ctrl+Z deshace un arrastre de una vez.
- Exportar en vertical + recorte: el MP4 sale 1080×1920 y cada bloque con su encuadre.
- Vertical + encajado: el visor muestra la composición 9:16 con fondo desenfocado; el export coincide.
- Marca de agua activada en vertical: aparece en su esquina del lienzo vertical.
- Compartir en vertical con un preset (p. ej. 10 MB): sale vertical y más pequeño (720×1280 o menos).
- Cerrar y reabrir el clip conserva el formato y los encuadres.
- Un clip ya vertical (p. ej. uno de TikTok) exportado en vertical + recorte sale entero.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-09-22-editor-redesign-design.md
git commit -F - <<'EOF'
Docs: vertical export composes with Direct2D and needs a GPU

The export already composes the watermark with Direct2D on the decoder's
device and the capture overlay already uses its Gaussian blur, so the
vertical stage does the same instead of a video processor plus a pixel
shader. There is no CPU path: capture itself requires a hardware D3D11
device.
EOF
```

---

## Cierre del plan

Al terminar: los tests de Rust y de Vitest, `pnpm check` y `pnpm build` pasan; el editor exporta y comparte en vertical con recorte por bloque o encajado, previsualizado en el visor. Con esto la spec del rediseño queda cubierta.
