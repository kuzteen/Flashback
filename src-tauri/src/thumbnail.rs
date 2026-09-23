// Genera una miniatura JPEG (un fotograma escalado) de un clip, para mostrarla como
// carátula ligera en la biblioteca sin cargar un elemento <video> por tarjeta. El trabajo
// pesado vive en el backend (CLAUDE.md §4): Media Foundation decodifica y escala el frame,
// y WIC lo codifica a JPEG. Sin dependencias externas.

#[cfg(target_os = "windows")]
pub use win::{capture, filmstrip, generate};

// Tira de fotogramas de la pista de vídeo del editor: una rejilla JPEG en memoria con un
// fotograma por keyframe y el instante real de cada uno. Nunca se escribe a disco.
pub struct Filmstrip {
    pub tile_w: u32,
    pub tile_h: u32,
    pub cols: u32,
    pub times_ms: Vec<f64>,
    pub jpeg: Vec<u8>,
}

impl Filmstrip {
    // Cabecera binaria (u32 LE: ancho, alto, columnas, n; luego n f64 LE) seguida del JPEG, para
    // cruzar el IPC como bytes crudos en vez de base64 dentro de JSON.
    pub fn into_bytes(self) -> Vec<u8> {
        let n = self.times_ms.len();
        let mut out = Vec::with_capacity(16 + n * 8 + self.jpeg.len());
        for v in [self.tile_w, self.tile_h, self.cols, n as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for t in &self.times_ms {
            out.extend_from_slice(&t.to_le_bytes());
        }
        out.extend_from_slice(&self.jpeg);
        out
    }
}

#[cfg(not(target_os = "windows"))]
pub fn generate(_src: String, _dst: String, _max_w: u32) -> Result<(), String> {
    Err("Las miniaturas solo están disponibles en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn filmstrip(_src: String, _tile_h: u32, _max_tiles: u32) -> Result<Filmstrip, String> {
    Err("La tira de fotogramas solo está disponible en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn capture(_src: String, _dst: String, _time_ms: f64) -> Result<(), String> {
    Err("Las capturas solo están disponibles en Windows".into())
}

#[cfg(target_os = "windows")]
mod win {
    use std::sync::Once;

    use windows::core::{Interface, GUID, HSTRING};
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng,
        GUID_WICPixelFormat24bppBGR, GUID_WICPixelFormat32bppBGRA, IWICBitmapFrameEncode,
        IWICImagingFactory, IWICStream, WICBitmapEncoderNoCache,
    };
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::StructuredStorage::{IPropertyBag2, PROPVARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IStream, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
        STREAM_SEEK_CUR,
    };
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};

    use super::Filmstrip;

    // Selector "primer flujo de vídeo" del SourceReader y flag de fin de stream.
    const FIRST_VIDEO: u32 = 0xFFFF_FFFC;
    const ENDOFSTREAM: u32 = 0x0000_0002;
    // GENERIC_WRITE, para IWICStream::InitializeFromFilename.
    const GENERIC_WRITE: u32 = 0x4000_0000;

    static MF_INIT: Once = Once::new();
    fn ensure_mf() {
        MF_INIT.call_once(|| unsafe {
            let _ = MFStartup(MF_VERSION, MFSTARTUP_LITE);
        });
    }

    pub fn generate(src: String, dst: String, max_w: u32) -> std::result::Result<(), String> {
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            ensure_mf();
            let r = run(&src, &dst, max_w, -1, &GUID_ContainerFormatJpeg).map_err(|e| format!("{e:?}"));
            unsafe { CoUninitialize() };
            r
        })
        .join()
        .map_err(|_| "El hilo de miniatura terminó inesperadamente".to_string())?
    }

    // Captura del fotograma mostrado: igual que la miniatura pero a resolución nativa (max_w 0) y
    // con el lector posicionado en el tiempo pedido.
    pub fn capture(src: String, dst: String, time_ms: f64) -> std::result::Result<(), String> {
        let seek_hns = (time_ms.max(0.0) * 10_000.0) as i64;
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            ensure_mf();
            let r = run(&src, &dst, 0, seek_hns, &GUID_ContainerFormatPng).map_err(|e| format!("{e:?}"));
            unsafe { CoUninitialize() };
            r
        })
        .join()
        .map_err(|_| "El hilo de captura terminó inesperadamente".to_string())?
    }

    pub fn filmstrip(src: String, tile_h: u32, max_tiles: u32) -> std::result::Result<Filmstrip, String> {
        std::thread::spawn(move || {
            // Trabajo de fondo mientras quizá se está jugando: que ceda ante el juego y la captura.
            unsafe {
                let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            ensure_mf();
            let r = run_film(&src, tile_h, max_tiles).map_err(|e| format!("{e:?}"));
            unsafe { CoUninitialize() };
            r
        })
        .join()
        .map_err(|_| "El hilo de la tira de fotogramas terminó inesperadamente".to_string())?
    }

    // Cada salto deja el lector en el keyframe anterior al instante pedido y solo se decodifica
    // ese fotograma: con un GOP de ~1 s, un fotograma por segundo sale casi gratis. Los saltos que
    // caen en el mismo keyframe se descartan para no repetir fotogramas.
    fn run_film(src: &str, tile_h: u32, max_tiles: u32) -> windows::core::Result<Filmstrip> {
        const COLS: u32 = 16;
        const MIN_STEP_HNS: i64 = 10_000_000;

        // Con device manager el lector decodifica por DXVA y reescala en la GPU; a la CPU solo baja
        // la miniatura ya reducida. Sin GPU utilizable se cae al mismo camino por software.
        let gpu = crate::editor::create_gpu();
        let mut attrs: Option<IMFAttributes> = None;
        unsafe { MFCreateAttributes(&mut attrs, 3)? };
        let attrs = attrs.unwrap();
        unsafe { attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)? };
        if let Some(g) = &gpu {
            unsafe { attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &g.manager.cast::<windows::core::IUnknown>()?)? };
        }
        // Sin baja latencia el decodificador retiene varios fotogramas tras cada salto antes de
        // entregar el keyframe: en un clip 1080p de 40 s la tira pasaba de 2,0 s a 3,6 s.
        unsafe { attrs.SetUINT32(&MF_LOW_LATENCY, 1)? };
        let reader = unsafe { MFCreateSourceReaderFromURL(&HSTRING::from(src), &attrs)? };

        let native = unsafe { reader.GetNativeMediaType(FIRST_VIDEO, 0)? };
        let size = unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(pack(1280, 720));
        let sw = ((size >> 32) as u32).max(1);
        let sh = ((size & 0xFFFF_FFFF) as u32).max(1);
        let th = tile_h.max(2) & !1;
        let tw = ((th as u64 * sw as u64 / sh as u64) as u32).max(2) & !1;

        let out = unsafe { MFCreateMediaType()? };
        unsafe {
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, pack(tw, th))?;
            reader.SetCurrentMediaType(FIRST_VIDEO, None, &out)?;
        }

        let pv = unsafe { reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)? };
        let dur = u64::try_from(&pv).unwrap_or(0) as i64;
        let step = (dur / max_tiles.max(1) as i64).max(MIN_STEP_HNS);

        let mut dims: Option<(u32, u32, i32)> = None;
        let mut tiles: Vec<Vec<u8>> = Vec::new();
        let mut times_ms: Vec<f64> = Vec::new();
        let mut last_ts = i64::MIN;
        let mut target = 0i64;
        while target < dur.max(1) {
            let pos = PROPVARIANT::from(target);
            unsafe { reader.SetCurrentPosition(&GUID::zeroed(), &pos)? };
            target += step;
            let mut sample: Option<IMFSample> = None;
            for _ in 0..8 {
                let mut flags = 0u32;
                unsafe { reader.ReadSample(FIRST_VIDEO, 0, None, Some(&mut flags), None, Some(&mut sample))? };
                if sample.is_some() || flags & ENDOFSTREAM != 0 {
                    break;
                }
            }
            let Some(s) = sample else { break };
            let ts = unsafe { s.GetSampleTime() }.unwrap_or(0);
            if ts <= last_ts {
                continue;
            }
            last_ts = ts;
            // MF puede ajustar el tipo al entregar el primer fotograma (sobre todo con GPU), así que
            // el tamaño y el stride reales se leen tras él y valen para toda la tira.
            let (aw, ah, ds) = match dims {
                Some(d) => d,
                None => {
                    let actual = unsafe { reader.GetCurrentMediaType(FIRST_VIDEO)? };
                    let asize = unsafe { actual.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(pack(tw, th));
                    let ds = unsafe { actual.GetUINT32(&MF_MT_DEFAULT_STRIDE) }.unwrap_or(0) as i32;
                    let d = (((asize >> 32) as u32).max(1), ((asize & 0xFFFF_FFFF) as u32).max(1), ds);
                    dims = Some(d);
                    d
                }
            };
            let row_bytes = (aw * 4) as usize;

            let buf = unsafe { s.ConvertToContiguousBuffer()? };
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            unsafe { buf.Lock(&mut ptr, None, Some(&mut len))? };
            let px = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            let src_stride = if ds != 0 {
                ds.unsigned_abs() as usize
            } else {
                (len as usize / ah as usize).max(row_bytes)
            };
            let mut tile = vec![0u8; row_bytes * ah as usize];
            for row in 0..ah as usize {
                let sr = if ds < 0 { ah as usize - 1 - row } else { row };
                let o = sr * src_stride;
                if o + row_bytes <= px.len() {
                    tile[row * row_bytes..(row + 1) * row_bytes].copy_from_slice(&px[o..o + row_bytes]);
                }
            }
            unsafe { buf.Unlock()? };
            tiles.push(tile);
            times_ms.push(ts as f64 / 10_000.0);
        }
        let Some((aw, ah, _)) = dims else {
            return Err(windows::core::Error::from(windows::core::HRESULT(0x8000_4005u32 as i32)));
        };
        let row_bytes = (aw * 4) as usize;

        let n = tiles.len() as u32;
        let cols = n.min(COLS);
        let rows = n.div_ceil(cols);
        let (gw, gh) = (aw * cols, ah * rows);
        let g_stride = (gw * 4) as usize;
        let mut grid = vec![0u8; g_stride * gh as usize];
        for (i, tile) in tiles.iter().enumerate() {
            let (cx, cy) = (i as u32 % cols, i as u32 / cols);
            for row in 0..ah as usize {
                let d = (cy as usize * ah as usize + row) * g_stride + cx as usize * row_bytes;
                grid[d..d + row_bytes].copy_from_slice(&tile[row * row_bytes..(row + 1) * row_bytes]);
            }
        }

        let jpeg = encode_jpeg_mem(gw, gh, g_stride as u32, &grid)?;
        Ok(Filmstrip { tile_w: aw, tile_h: ah, cols, times_ms, jpeg })
    }

    // JPEG directo a memoria: un IWICStream sobre un buffer propio con holgura (un JPEG nunca
    // ocupa más que los píxeles en crudo); la posición final del stream da los bytes escritos.
    fn encode_jpeg_mem(w: u32, h: u32, stride: u32, pixels: &[u8]) -> windows::core::Result<Vec<u8>> {
        let factory: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };
        let mut mem = vec![0u8; pixels.len() + 64 * 1024];
        let stream = unsafe { factory.CreateStream()? };
        unsafe { stream.InitializeFromMemory(&mut mem)? };
        write_frame(&factory, &stream, w, h, stride, pixels, &GUID_ContainerFormatJpeg)?;
        let mut end = 0u64;
        unsafe { stream.cast::<IStream>()?.Seek(0, STREAM_SEEK_CUR, Some(&mut end))? };
        drop(stream);
        mem.truncate(end as usize);
        Ok(mem)
    }

    fn run(src: &str, dst: &str, max_w: u32, seek_hns: i64, container: &GUID) -> windows::core::Result<()> {
        // El SourceReader con procesamiento de vídeo avanzado puede entregar el frame ya
        // convertido a RGB32 y escalado al tamaño pedido, sin pasos manuales de color/escala.
        let mut attrs: Option<IMFAttributes> = None;
        unsafe { MFCreateAttributes(&mut attrs, 1)? };
        let attrs = attrs.unwrap();
        unsafe { attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)? };

        let url = HSTRING::from(src);
        let reader = unsafe { MFCreateSourceReaderFromURL(&url, &attrs)? };

        let native = unsafe { reader.GetNativeMediaType(FIRST_VIDEO, 0)? };
        let size = unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(pack(1280, 720));
        let sw = (size >> 32) as u32;
        let sh = (size & 0xFFFF_FFFF) as u32;
        let (tw, th) = target_size(sw.max(1), sh.max(1), max_w);

        let out = unsafe { MFCreateMediaType()? };
        unsafe {
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, pack(tw, th))?;
            reader.SetCurrentMediaType(FIRST_VIDEO, None, &out)?;
        }

        // El procesador de vídeo puede no escalar exactamente al tamaño pedido; tomamos las
        // dimensiones y el stride realmente negociados para no cizallar la imagen.
        let actual = unsafe { reader.GetCurrentMediaType(FIRST_VIDEO)? };
        let asize = unsafe { actual.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(pack(tw, th));
        let aw = ((asize >> 32) as u32).max(1);
        let ah = ((asize & 0xFFFF_FFFF) as u32).max(1);
        let ds = unsafe { actual.GetUINT32(&MF_MT_DEFAULT_STRIDE) }.unwrap_or(0) as i32;

        // Captura del frame actual: posicionar el lector en el tiempo pedido (unidades de 100ns,
        // formato GUID_NULL) antes de leer. seek_hns < 0 = primer frame (miniatura).
        if seek_hns >= 0 {
            let pos = PROPVARIANT::from(seek_hns);
            unsafe { reader.SetCurrentPosition(&GUID::zeroed(), &pos)? };
        }

        // Tras el seek, MF deja el lector en el keyframe <= tiempo pedido; hay que AVANZAR
        // decodificando hasta el fotograma realmente mostrado en `seek_hns` (el de mayor PTS
        // <= objetivo), o la captura saldría hasta un GOP atrasada respecto al vídeo pausado.
        // Para miniatura (seek_hns < 0) basta el primer frame con datos.
        const EPS_HNS: i64 = 20_000; // 2 ms de tolerancia al redondeo de currentTime
        let mut frame: Option<IMFSample> = None;
        for _ in 0..2000 {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            unsafe {
                reader.ReadSample(
                    FIRST_VIDEO,
                    0,
                    None,
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                )?
            };
            if flags & ENDOFSTREAM != 0 {
                break;
            }
            let Some(s) = sample else { continue };
            if seek_hns < 0 {
                frame = Some(s);
                break;
            }
            let ts = unsafe { s.GetSampleTime() }.unwrap_or(0);
            if ts <= seek_hns + EPS_HNS {
                // Aún en/antes del objetivo: guardar y seguir avanzando.
                frame = Some(s);
            } else {
                // Pasado el objetivo: el frame mostrado es el anterior ya guardado; si el
                // primero ya va pasado (seek cayó después), usar este.
                if frame.is_none() {
                    frame = Some(s);
                }
                break;
            }
        }
        let sample = frame.ok_or_else(|| {
            windows::core::Error::from(windows::core::HRESULT(0x8000_4005u32 as i32))
        })?;

        let buf = unsafe { sample.ConvertToContiguousBuffer()? };
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        unsafe { buf.Lock(&mut ptr, None, Some(&mut len))? };
        let src = unsafe { std::slice::from_raw_parts(ptr, len as usize) };

        // Empaquetamos a un buffer top-down y sin relleno (stride = aw*4). El stride origen
        // sale del MF_MT_DEFAULT_STRIDE (su signo indica orientación) o, en su defecto, de
        // len/altura. Copiar fila a fila evita cizallado por relleno y arregla bottom-up.
        let dst_stride = (aw * 4) as usize;
        let src_stride = if ds != 0 {
            ds.unsigned_abs() as usize
        } else {
            (len as usize / ah as usize).max(dst_stride)
        };
        let bottom_up = ds < 0;
        let mut packed = vec![0u8; dst_stride * ah as usize];
        for row in 0..ah as usize {
            let src_row = if bottom_up { ah as usize - 1 - row } else { row };
            let s = src_row * src_stride;
            let d = row * dst_stride;
            if s + dst_stride <= src.len() {
                packed[d..d + dst_stride].copy_from_slice(&src[s..s + dst_stride]);
            }
        }
        unsafe { buf.Unlock()? };

        encode_image(dst, aw, ah, dst_stride as u32, &packed, container)
    }

    fn encode_image(
        dst: &str,
        w: u32,
        h: u32,
        stride: u32,
        pixels: &[u8],
        container: &GUID,
    ) -> windows::core::Result<()> {
        let factory: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)? };

        let stream = unsafe { factory.CreateStream()? };
        unsafe { stream.InitializeFromFilename(&HSTRING::from(dst), GENERIC_WRITE)? };
        write_frame(&factory, &stream, w, h, stride, pixels, container)
    }

    fn write_frame(
        factory: &IWICImagingFactory,
        stream: &IWICStream,
        w: u32,
        h: u32,
        stride: u32,
        pixels: &[u8],
        container: &GUID,
    ) -> windows::core::Result<()> {
        let encoder = unsafe { factory.CreateEncoder(container, std::ptr::null())? };
        unsafe { encoder.Initialize(stream, WICBitmapEncoderNoCache)? };

        let mut frame: Option<IWICBitmapFrameEncode> = None;
        let mut props: Option<IPropertyBag2> = None;
        unsafe { encoder.CreateNewFrame(&mut frame, &mut props)? };
        let frame = frame.unwrap();
        unsafe { frame.Initialize(props.as_ref())? };
        unsafe { frame.SetSize(w, h)? };

        // JPEG no admite alfa, así que el encoder fija un formato sin alfa (24bpp). En vez de
        // escribir bytes crudos (que se desalinearían 4→3), entregamos un IWICBitmap declarado
        // como 32bppBGRA y dejamos que WIC convierta al formato del frame en WriteSource.
        let mut fmt: GUID = GUID_WICPixelFormat24bppBGR;
        unsafe { frame.SetPixelFormat(&mut fmt)? };

        let bitmap = unsafe {
            factory.CreateBitmapFromMemory(w, h, &GUID_WICPixelFormat32bppBGRA, stride, pixels)?
        };
        unsafe { frame.WriteSource(&bitmap, std::ptr::null())? };

        unsafe { frame.Commit()? };
        unsafe { encoder.Commit()? };
        Ok(())
    }

    // Escala manteniendo la relación de aspecto, sin superar `max_w` (0 = resolución nativa,
    // sin límite), con lados pares.
    fn target_size(sw: u32, sh: u32, max_w: u32) -> (u32, u32) {
        let w = if max_w == 0 { sw } else { sw.min(max_w) };
        let h = ((w as u64 * sh as u64) / sw as u64) as u32;
        (w & !1, h.max(2) & !1)
    }

    fn pack(hi: u32, lo: u32) -> u64 {
        (hi as u64) << 32 | lo as u64
    }
}
