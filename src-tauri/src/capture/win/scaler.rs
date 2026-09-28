use std::mem::ManuallyDrop;
use std::sync::Mutex;

use windows::core::{Interface, Result, BOOL};
use windows::Win32::Foundation::{E_INVALIDARG, E_NOTIMPL, E_POINTER, RECT};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

// Conversión de color y escalado en una sola pasada del procesador de vídeo de D3D11. En la
// captura lleva cada frame BGRA de WGC directo a NV12 a resolución de salida, en el anillo que
// consume el encoder: sustituye a copiarlo en BGRA a nativo y convertirlo después con el MFT, así
// que hay una pasada de memoria en vez de dos y el anillo ocupa 1,5 B/px a resolución de salida
// en vez de 4 B/px a nativo (a 4K eran ~1 GB de VRAM). La salida es idéntica bit a bit a la del
// MFT (lo fija un test).
pub(super) struct VideoBlt {
    inner: Mutex<Inner>,
}

struct Inner {
    vdev: ID3D11VideoDevice,
    vctx: ID3D11VideoContext,
    enumr: ID3D11VideoProcessorEnumerator,
    proc: ID3D11VideoProcessor,
    outs: Vec<(ID3D11Texture2D, ID3D11VideoProcessorOutputView)>,
    // WGC recicla las texturas de su frame pool (2): se guarda la vista de cada una. Se retiene
    // la textura para que su puntero no pueda reaparecer en otro objeto mientras siga aquí.
    ins: Vec<(ID3D11Texture2D, ID3D11VideoProcessorInputView)>,
}

// Solo se usa bajo el Mutex, sobre un device multihilo-protegido.
unsafe impl Send for Inner {}

const MAX_INPUT_VIEWS: usize = 4;

fn err(code: windows::core::HRESULT) -> windows::core::Error {
    windows::core::Error::from_hresult(code)
}

// Espacio de color YUV que elige el conversor MFT al que sustituye: sin MF_MT_YUV_MATRIX, Media
// Foundation usa BT.709 en HD y BT.601 por debajo, en rango de estudio.
pub(super) fn yuv_color_space(height: u32) -> DXGI_COLOR_SPACE_TYPE {
    if height >= 720 {
        DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709
    } else {
        DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601
    }
}

pub(super) const RGB_FULL: DXGI_COLOR_SPACE_TYPE = DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709;

impl VideoBlt {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        device: &ID3D11Device,
        ctx: &ID3D11DeviceContext,
        (in_w, in_h): (u32, u32),
        (out_w, out_h): (u32, u32),
        fps: u32,
        in_cs: DXGI_COLOR_SPACE_TYPE,
        out_cs: DXGI_COLOR_SPACE_TYPE,
        out_format: DXGI_FORMAT,
    ) -> Result<VideoBlt> {
        let vdev: ID3D11VideoDevice = device.cast()?;
        let vctx: ID3D11VideoContext = ctx.cast()?;
        let rate = DXGI_RATIONAL { Numerator: fps.max(1), Denominator: 1 };
        let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: rate,
            InputWidth: in_w,
            InputHeight: in_h,
            OutputFrameRate: rate,
            OutputWidth: out_w,
            OutputHeight: out_h,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        unsafe {
            let enumr = vdev.CreateVideoProcessorEnumerator(&desc)?;
            let flags = enumr.CheckVideoProcessorFormat(out_format)?;
            if flags & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 == 0 {
                return Err(err(E_NOTIMPL));
            }
            let proc = vdev.CreateVideoProcessor(&enumr, 0)?;
            let vctx1: ID3D11VideoContext1 = vctx.cast()?;
            vctx1.VideoProcessorSetStreamColorSpace1(&proc, 0, in_cs);
            vctx1.VideoProcessorSetOutputColorSpace1(&proc, out_cs);
            vctx.VideoProcessorSetStreamFrameFormat(&proc, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            vctx.VideoProcessorSetStreamAutoProcessingMode(&proc, 0, false);
            let full = RECT { left: 0, top: 0, right: in_w as i32, bottom: in_h as i32 };
            vctx.VideoProcessorSetStreamSourceRect(&proc, 0, true, Some(&full));
            let dst = RECT { left: 0, top: 0, right: out_w as i32, bottom: out_h as i32 };
            vctx.VideoProcessorSetStreamDestRect(&proc, 0, true, Some(&dst));
            vctx.VideoProcessorSetOutputTargetRect(&proc, true, Some(&dst));
            Ok(VideoBlt {
                inner: Mutex::new(Inner { vdev, vctx, enumr, proc, outs: Vec::new(), ins: Vec::new() }),
            })
        }
    }

    // Registra las texturas de destino; se hace una vez al montar el pipeline.
    pub(super) fn add_targets(&self, targets: &[ID3D11Texture2D]) -> Result<()> {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        for tex in targets {
            let mut view = None;
            unsafe { g.vdev.CreateVideoProcessorOutputView(tex, &g.enumr, &desc, Some(&mut view))? };
            let view = view.ok_or_else(|| err(E_POINTER))?;
            g.outs.push((tex.clone(), view));
        }
        Ok(())
    }

    pub(super) fn blt(&self, src: &ID3D11Texture2D, dst: &ID3D11Texture2D) -> Result<()> {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let out = g
            .outs
            .iter()
            .find(|(t, _)| t.as_raw() == dst.as_raw())
            .map(|(_, v)| v.clone())
            .ok_or_else(|| err(E_INVALIDARG))?;
        let input = match g.ins.iter().find(|(t, _)| t.as_raw() == src.as_raw()) {
            Some((_, v)) => v.clone(),
            None => {
                let desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                    FourCC: 0,
                    ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                        Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 },
                    },
                };
                let mut view = None;
                unsafe { g.vdev.CreateVideoProcessorInputView(src, &g.enumr, &desc, Some(&mut view))? };
                let view = view.ok_or_else(|| err(E_POINTER))?;
                if g.ins.len() >= MAX_INPUT_VIEWS {
                    g.ins.remove(0);
                }
                g.ins.push((src.clone(), view.clone()));
                view
            }
        };
        let stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: BOOL(1),
            pInputSurface: ManuallyDrop::new(Some(input)),
            ..Default::default()
        };
        let mut streams = [stream];
        let r = unsafe { g.vctx.VideoProcessorBlt(&g.proc, &out, 0, &streams) };
        unsafe { ManuallyDrop::drop(&mut streams[0].pInputSurface) };
        r
    }
}

pub(super) fn create_nv12_targets(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    count: usize,
) -> Result<Vec<ID3D11Texture2D>> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut t: Option<ID3D11Texture2D> = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut t))? };
        out.push(t.ok_or_else(|| err(E_POINTER))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::MediaFoundation::*;

    fn bgra_pattern(device: &ID3D11Device, w: u32, h: u32) -> ID3D11Texture2D {
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let band = x * 8 / w;
                let (r, g, b) = match band {
                    0 => (255, 0, 0),
                    1 => (0, 255, 0),
                    2 => (0, 0, 255),
                    3 => (255, 255, 255),
                    4 => (16, 16, 16),
                    5 => ((y * 255 / h) as u8, 128, 40),
                    6 => (((x ^ y) & 1) as u8 * 255, 90, 200),
                    _ => (200, 150, (x * 255 / w) as u8),
                };
                px[i] = b;
                px[i + 1] = g;
                px[i + 2] = r;
                px[i + 3] = 255;
            }
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let init = D3D11_SUBRESOURCE_DATA { pSysMem: px.as_ptr() as _, SysMemPitch: w * 4, SysMemSlicePitch: 0 };
        let mut t = None;
        unsafe { device.CreateTexture2D(&desc, Some(&init), Some(&mut t)).unwrap() };
        t.unwrap()
    }

    fn read_nv12(device: &ID3D11Device, src: &ID3D11Texture2D, sub: u32, w: u32, h: u32) -> Vec<u8> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut st = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut st)).unwrap() };
        let st = st.unwrap();
        let ctx = unsafe { device.GetImmediateContext().unwrap() };
        let mut out = Vec::with_capacity((w * h * 3 / 2) as usize);
        unsafe {
            ctx.CopySubresourceRegion(&st, 0, 0, 0, 0, src, sub, None);
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(&st, 0, D3D11_MAP_READ, 0, Some(&mut map)).unwrap();
            let base = map.pData as *const u8;
            let pitch = map.RowPitch as usize;
            for row in 0..(h + h / 2) as usize {
                out.extend_from_slice(std::slice::from_raw_parts(base.add(row * pitch), w as usize));
            }
            ctx.Unmap(&st, 0);
        }
        out
    }

    fn sample_of(tex: &ID3D11Texture2D) -> IMFSample {
        unsafe {
            let s = MFCreateSample().unwrap();
            s.AddBuffer(&super::super::surface_buffer(tex).unwrap()).unwrap();
            s
        }
    }

    fn via_mft(device: &ID3D11Device, bgra: &ID3D11Texture2D, iw: u32, ih: u32, ow: u32, oh: u32) -> Vec<u8> {
        super::super::ensure_mf();
        let mut token = 0u32;
        let mut manager = None;
        unsafe { MFCreateDXGIDeviceManager(&mut token, &mut manager).unwrap() };
        let manager: IMFDXGIDeviceManager = manager.unwrap();
        unsafe { manager.ResetDevice(device, token).unwrap() };
        let conv = super::super::encoder::build_converter(Some(&manager), iw, ih, ow, oh, 60).unwrap();
        let provides = unsafe { conv.GetOutputStreamInfo(0).unwrap().dwFlags }
            & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32
            != 0;
        let target = create_nv12_targets(device, ow, oh, 1).unwrap().remove(0);
        unsafe {
            conv.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0).unwrap();
            let input = sample_of(bgra);
            conv.ProcessInput(0, &input, 0).unwrap();
            let mut out = MFT_OUTPUT_DATA_BUFFER::default();
            if !provides {
                out.pSample = ManuallyDrop::new(Some(sample_of(&target)));
            }
            let mut status = 0u32;
            conv.ProcessOutput(0, std::slice::from_mut(&mut out), &mut status).unwrap();
            let sample = ManuallyDrop::take(&mut out.pSample).unwrap();
            let buf: IMFDXGIBuffer = sample.GetBufferByIndex(0).unwrap().cast().unwrap();
            let mut tex: Option<ID3D11Texture2D> = None;
            buf.GetResource(&ID3D11Texture2D::IID, &mut tex as *mut _ as *mut _).unwrap();
            let sub = buf.GetSubresourceIndex().unwrap();
            read_nv12(device, &tex.unwrap(), sub, ow, oh)
        }
    }

    fn via_scaler(device: &ID3D11Device, bgra: &ID3D11Texture2D, iw: u32, ih: u32, ow: u32, oh: u32) -> Vec<u8> {
        let ctx = unsafe { device.GetImmediateContext().unwrap() };
        let s = VideoBlt::new(device, &ctx, (iw, ih), (ow, oh), 60, RGB_FULL, yuv_color_space(oh), DXGI_FORMAT_NV12).unwrap();
        let target = create_nv12_targets(device, ow, oh, 1).unwrap().remove(0);
        s.add_targets(std::slice::from_ref(&target)).unwrap();
        s.blt(bgra, &target).unwrap();
        read_nv12(device, &target, 0, ow, oh)
    }

    fn compare(iw: u32, ih: u32, ow: u32, oh: u32) -> (u8, f64) {
        let (device, _) = super::super::create_device().unwrap();
        let bgra = bgra_pattern(&device, iw, ih);
        let a = via_mft(&device, &bgra, iw, ih, ow, oh);
        let b = via_scaler(&device, &bgra, iw, ih, ow, oh);
        let max = a.iter().zip(&b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        let mean = a.iter().zip(&b).map(|(x, y)| x.abs_diff(*y) as f64).sum::<f64>() / a.len() as f64;
        eprintln!("{iw}x{ih} -> {ow}x{oh}: diff max {max}, media {mean:.3}");
        (max, mean)
    }

    fn read_bgra_px(device: &ID3D11Device, src: &ID3D11Texture2D, w: u32, h: u32, x: u32, y: u32) -> [u8; 4] {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut st = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut st)).unwrap() };
        let st = st.unwrap();
        let ctx = unsafe { device.GetImmediateContext().unwrap() };
        unsafe {
            ctx.CopyResource(&st, src);
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(&st, 0, D3D11_MAP_READ, 0, Some(&mut map)).unwrap();
            let p = (map.pData as *const u8).add(y as usize * map.RowPitch as usize + x as usize * 4);
            let px = [*p, *p.add(1), *p.add(2), *p.add(3)];
            ctx.Unmap(&st, 0);
            px
        }
    }

    // El cartel de "fuera de foco" se compone en BGRA a partir del último frame NV12 del anillo.
    #[test]
    fn nv12_ring_frame_goes_back_to_bgra() {
        let (device, _) = super::super::create_device().unwrap();
        let ctx = unsafe { device.GetImmediateContext().unwrap() };
        let (w, h) = (1280, 720);
        let bgra = bgra_pattern(&device, w, h);
        let nv12 = create_nv12_targets(&device, w, h, 1).unwrap().remove(0);
        let fwd = VideoBlt::new(&device, &ctx, (w, h), (w, h), 60, RGB_FULL, yuv_color_space(h), DXGI_FORMAT_NV12).unwrap();
        fwd.add_targets(std::slice::from_ref(&nv12)).unwrap();
        fwd.blt(&bgra, &nv12).unwrap();
        let back_tex = super::super::create_bgra_textures(&device, w, h, 1).unwrap().remove(0);
        let back = VideoBlt::new(&device, &ctx, (w, h), (w, h), 60, yuv_color_space(h), RGB_FULL, DXGI_FORMAT_B8G8R8A8_UNORM).unwrap();
        back.add_targets(std::slice::from_ref(&back_tex)).unwrap();
        back.blt(&nv12, &back_tex).unwrap();
        let red = read_bgra_px(&device, &back_tex, w, h, w / 16, h / 2);
        assert!(red[2] > 230 && red[1] < 25 && red[0] < 25, "rojo: {red:?}");
    }

    #[test]
    fn matches_the_mft_converter() {
        for (iw, ih, ow, oh) in [(1920, 1080, 1920, 1080), (2560, 1440, 1920, 1080), (1920, 1080, 1280, 720), (1280, 720, 854, 480), (854, 480, 854, 480)] {
            let (_, mean) = compare(iw, ih, ow, oh);
            assert!(mean < 1.0, "{iw}x{ih} -> {ow}x{oh}: media {mean}");
        }
    }
}
