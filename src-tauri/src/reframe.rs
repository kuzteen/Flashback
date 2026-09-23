// Formato de salida del export y geometría del encuadre vertical. La geometría es pura y se prueba
// en cualquier plataforma; la composición en GPU vive en el submódulo de Windows.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Fill {
    #[default]
    Crop,
    Fit,
    Custom,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Horizontal,
    Vertical {
        fill: Fill,
        // Solo cuenta con `Custom`: los otros dos rellenos son sus extremos.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        zoom: Option<f32>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

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

// Encajado y recorte son los extremos de un mismo control: 0 = el fotograma entero cabe, 1 = llena
// el lienzo. Personalizado queda entre medias.
pub fn zoom_of(fill: Fill, zoom: Option<f32>) -> f32 {
    match fill {
        Fill::Fit => 0.0,
        Fill::Crop => 1.0,
        Fill::Custom => zoom.unwrap_or(0.5).clamp(0.0, 1.0),
    }
}

// Dónde se dibuja el fotograma ENTERO dentro del lienzo de salida; puede salirse, y lo que se sale
// no se ve. En cada eje, si el fotograma es mayor que el lienzo, la posición desplaza qué parte se
// ve (0..1 sobre el origen); si es menor, coloca el fotograma dentro del lienzo (0..1 sobre el
// lienzo). La misma cuenta está en el frontend para que la previsualización sea lo exportado.
pub fn place(src_w: u32, src_h: u32, out_w: u32, out_h: u32, zoom: f32, cx: f64, cy: f64) -> Rect {
    let (sw, sh, ow, oh) = (src_w as f32, src_h as f32, out_w as f32, out_h as f32);
    let fit = (ow / sw).min(oh / sh);
    let fill = (ow / sw).max(oh / sh);
    let s = fit + (fill - fit) * zoom.clamp(0.0, 1.0);
    let (w, h) = (sw * s, sh * s);
    Rect { x: axis(w, ow, cx), y: axis(h, oh, cy), w, h }
}

fn axis(size: f32, canvas: f32, pos: f64) -> f32 {
    let p = pos.clamp(0.0, 1.0) as f32;
    if size > canvas {
        (canvas / 2.0 - p * size).clamp(canvas - size, 0.0)
    } else {
        (p * canvas - size / 2.0).clamp(0.0, canvas - size)
    }
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

    fn close(a: Rect, b: Rect) {
        let ok = (a.x - b.x).abs() < 0.01
            && (a.y - b.y).abs() < 0.01
            && (a.w - b.w).abs() < 0.01
            && (a.h - b.h).abs() < 0.01;
        assert!(ok, "{a:?} != {b:?}");
    }

    #[test]
    fn the_fills_map_to_the_ends_of_the_zoom() {
        assert_eq!(zoom_of(Fill::Fit, Some(0.7)), 0.0);
        assert_eq!(zoom_of(Fill::Crop, Some(0.7)), 1.0);
        assert_eq!(zoom_of(Fill::Custom, Some(0.7)), 0.7);
        assert_eq!(zoom_of(Fill::Custom, None), 0.5);
        assert_eq!(zoom_of(Fill::Custom, Some(3.0)), 1.0);
    }

    #[test]
    fn zoom_zero_fits_the_whole_frame_centred() {
        close(place(1920, 1080, 1080, 1920, 0.0, 0.5, 0.5), r(0.0, 656.25, 1080.0, 607.5));
    }

    #[test]
    fn zoom_one_fills_the_canvas_and_pans_with_the_horizontal_position() {
        close(place(1920, 1080, 1080, 1920, 1.0, 0.5, 0.5), r(-1166.67, 0.0, 3413.33, 1920.0));
        close(place(1920, 1080, 1080, 1920, 1.0, 0.0, 0.5), r(0.0, 0.0, 3413.33, 1920.0));
        close(place(1920, 1080, 1080, 1920, 1.0, 1.0, 0.5), r(-2333.33, 0.0, 3413.33, 1920.0));
    }

    #[test]
    fn a_middle_zoom_crops_the_sides_and_leaves_bands_above_and_below() {
        close(place(1920, 1080, 1080, 1920, 0.5, 0.5, 0.5), r(-583.33, 328.13, 2246.67, 1263.75));
    }

    #[test]
    fn the_vertical_position_moves_a_band_inside_the_canvas() {
        close(place(1920, 1080, 1080, 1920, 0.0, 0.5, 0.0), r(0.0, 0.0, 1080.0, 607.5));
        close(place(1920, 1080, 1080, 1920, 0.0, 0.5, 1.0), r(0.0, 1312.5, 1080.0, 607.5));
    }

    #[test]
    fn an_already_vertical_source_fills_the_canvas_at_any_zoom() {
        close(place(1080, 1920, 1080, 1920, 0.0, 0.3, 0.8), r(0.0, 0.0, 1080.0, 1920.0));
        close(place(1080, 1920, 1080, 1920, 1.0, 0.3, 0.8), r(0.0, 0.0, 1080.0, 1920.0));
    }

    #[test]
    fn formats_read_and_write_the_frontend_shape() {
        let v: OutputFormat = serde_json::from_str(r#"{"kind":"vertical","fill":"fit"}"#).unwrap();
        assert_eq!(v, OutputFormat::Vertical { fill: Fill::Fit, zoom: None });
        let c: OutputFormat =
            serde_json::from_str(r#"{"kind":"vertical","fill":"custom","zoom":0.4}"#).unwrap();
        assert_eq!(c, OutputFormat::Vertical { fill: Fill::Custom, zoom: Some(0.4) });
        let h: OutputFormat = serde_json::from_str(r#"{"kind":"horizontal"}"#).unwrap();
        assert_eq!(h, OutputFormat::Horizontal);
        assert_eq!(
            serde_json::to_string(&OutputFormat::Vertical { fill: Fill::Crop, zoom: None }).unwrap(),
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

#[cfg(target_os = "windows")]
pub mod win {
    use super::{place, Rect};
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
        bg_bmp: ID2D1Bitmap1,
        blur: ID2D1Effect,
        dim: ID2D1SolidColorBrush,
        allocator: IMFVideoSampleAllocatorEx,
        out_type: IMFMediaType,
        // Un bitmap destino por textura del asignador: crearlos por fotograma costaba sin motivo.
        targets: RefCell<HashMap<usize, ID2D1Bitmap1>>,
        zoom: f32,
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
            zoom: f32,
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

            // Fondo desenfocado siempre preparado: con zoom intermedio o encajado se ve arriba y
            // abajo, y solo sobra cuando el fotograma cubre el lienzo entero.
            let bg_bmp = unsafe {
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
            let allocator: IMFVideoSampleAllocatorEx = unsafe {
                let mut raw: *mut std::ffi::c_void = std::ptr::null_mut();
                MFCreateVideoSampleAllocatorEx(&IMFVideoSampleAllocatorEx::IID, &mut raw)?;
                if raw.is_null() {
                    return Err(windows::core::Error::from(E_POINTER));
                }
                IMFVideoSampleAllocatorEx::from_raw(raw)
            };
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
                zoom,
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

        pub fn process(&self, input: &IMFSample, crop_x: f64, crop_y: f64) -> Result<IMFSample> {
            self.load_input(input)?;
            let out = self.next_sample()?;
            let target = self.target_for(&out)?;
            let (ow, oh) = (self.out_w as f32, self.out_h as f32);
            let full = D2D_RECT_F { left: 0.0, top: 0.0, right: ow, bottom: oh };
            let fg = place(self.src_w, self.src_h, self.out_w, self.out_h, self.zoom, crop_x, crop_y);
            let covers = fg.x <= 0.0 && fg.y <= 0.0 && fg.x + fg.w >= ow && fg.y + fg.h >= oh;
            unsafe {
                if !covers {
                    // Fondo: el fotograma llenando el lienzo, desenfocado y velado para que el de
                    // delante destaque.
                    let cover = d2d(place(self.src_w, self.src_h, self.out_w, self.out_h, 1.0, 0.5, 0.5));
                    self.ctx.SetTarget(&self.bg_bmp);
                    self.ctx.BeginDraw();
                    self.ctx.DrawBitmap(&self.src_bmp, Some(&cover), 1.0, D2D1_INTERPOLATION_MODE_LINEAR, None, None);
                    self.ctx.EndDraw(None, None)?;
                }
                self.ctx.SetTarget(&target);
                self.ctx.BeginDraw();
                if !covers {
                    self.blur.SetInput(0, &self.bg_bmp, true);
                    let img = self.blur.GetOutput()?;
                    self.ctx.DrawImage(&img, None, None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
                    self.ctx.FillRectangle(&full, &self.dim);
                }
                // El fotograma entero en su sitio: lo que cae fuera del lienzo lo recorta el destino.
                self.ctx.DrawBitmap(&self.src_bmp, Some(&d2d(fg)), 1.0, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC, None, None);
                self.ctx.EndDraw(None, None)?;
                self.ctx.SetTarget(None);
            }
            // Las muestras del asignador nacen con longitud 0 y el sink rechaza una muestra vacía
            // (E_INVALIDARG en WriteSample): la textura ya está llena, así que ocupa el búfer entero.
            let buf = unsafe { out.GetBufferByIndex(0)? };
            unsafe { buf.SetCurrentLength(buf.GetMaxLength()?)? };
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
