use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Default)]
pub struct ClipAudio {
    pub system: Option<String>,
    pub mic: Option<String>,
    // Forma de onda ya reducida a cubos: se calcula en el backend para evitar volcar el WAV
    // completo al WebView y decodificarlo allí (cientos de MB). Solo viaja el envolvente.
    pub sys_peaks: Option<Vec<f32>>,
    pub mic_peaks: Option<Vec<f32>>,
    pub mix_peaks: Option<Vec<f32>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixerState {
    pub sys_vol: f32,
    pub sys_muted: bool,
    pub mic_vol: f32,
    pub mic_muted: bool,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            sys_vol: 1.0,
            sys_muted: false,
            mic_vol: 1.0,
            mic_muted: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Segment {
    pub start_ms: f64,
    pub end_ms: f64,
    // Estado del editor (posición en la línea de tiempo y tamaño máximo de la sección). La
    // exportación no los usa —une los tramos sin huecos— pero se persisten para restaurar el
    // montaje. Opcionales por compatibilidad con ediciones antiguas.
    #[serde(default)]
    pub pos_ms: Option<f64>,
    #[serde(default)]
    pub bound_start_ms: Option<f64>,
    #[serde(default)]
    pub bound_end_ms: Option<f64>,
    // Solo se persiste para restaurar el montaje; la exportación recibe ya filtrados los activos.
    #[serde(default)]
    pub disabled: Option<bool>,
    // Centro horizontal del marco 9:16 del formato vertical (0..1). Sin él, centrado.
    #[serde(default)]
    pub crop_x: Option<f64>,
    // Centro vertical, igual que crop_x: desplaza qué parte se ve o coloca la franja en el lienzo.
    #[serde(default)]
    pub crop_y: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClipEdit {
    pub segments: Vec<Segment>,
    pub mixer: MixerState,
    #[serde(default)]
    pub format: crate::reframe::OutputFormat,
    #[serde(default)]
    pub look: crate::look::Look,
}

#[cfg(target_os = "windows")]
pub(crate) use win::create_gpu;
#[cfg(target_os = "windows")]
pub use win::{
    clip_dims, clip_fps, export_clip, frame_times, keyframe_times, prepare_clip_audio,
};

// Edición no destructiva: cortes y mezcla viven en el índice único de app-data (no en un sidecar
// por clip), indexados por la ruta del MP4. El original nunca se toca. No usa Media Foundation,
// así que es común a todas las plataformas.
// Abrir un clip en el editor y cerrarlo persiste el montaje aunque no se haya tocado nada. Un
// montaje que no recorta, no desactiva nada y no cambia la mezcla no es una edición: se borra la
// entrada en vez de guardarla, para que la biblioteca no marque como editados clips intactos.
fn is_noop(edit: &ClipEdit) -> bool {
    if edit.format != crate::reframe::OutputFormat::Horizontal || !edit.look.is_neutral() {
        return false;
    }
    let m = &edit.mixer;
    if m.sys_muted
        || m.mic_muted
        || (m.sys_vol - 1.0).abs() > f32::EPSILON
        || (m.mic_vol - 1.0).abs() > f32::EPSILON
    {
        return false;
    }
    match edit.segments.as_slice() {
        [] => true,
        // Sin los límites originales (ediciones antiguas) no hay forma de saber si recorta, y se
        // prefiere marcarlo de más: perder una edición real sería peor.
        [s] => {
            !s.disabled.unwrap_or(false)
                && matches!(
                    (s.bound_start_ms, s.bound_end_ms),
                    (Some(bs), Some(be))
                        if (s.start_ms - bs).abs() < 1.0 && (s.end_ms - be).abs() < 1.0
                )
        }
        _ => false,
    }
}

pub fn save_edit(index: String, path: String, edit: ClipEdit) -> Result<(), String> {
    let idx = std::path::Path::new(&index);
    if is_noop(&edit) {
        crate::edits::remove(idx, &path);
        return Ok(());
    }
    let val = serde_json::to_value(&edit).map_err(|e| e.to_string())?;
    crate::edits::save(idx, &path, val);
    Ok(())
}

// Rutas con un montaje real guardado. Filtra las entradas vacías que dejaron las versiones que
// persistían al cerrar el editor sin haber tocado nada.
pub fn edited_paths(index: &std::path::Path) -> Vec<String> {
    crate::edits::entries(index)
        .into_iter()
        .filter(|(_, v)| {
            serde_json::from_value::<ClipEdit>(v.clone()).map_or(true, |e| !is_noop(&e))
        })
        .map(|(k, _)| k)
        .collect()
}

pub fn load_edit(index: String, path: String) -> Result<ClipEdit, String> {
    let idx = std::path::Path::new(&index);
    if let Some(val) = crate::edits::load(idx, &path) {
        return serde_json::from_value(val).map_err(|e| e.to_string());
    }
    // Migración: importar el viejo sidecar `<clip>.edit.json` al índice y borrarlo.
    let legacy = std::path::Path::new(&path).with_extension("edit.json");
    if let Ok(s) = std::fs::read_to_string(&legacy) {
        if let Ok(edit) = serde_json::from_str::<ClipEdit>(&s) {
            if let Ok(val) = serde_json::to_value(&edit) {
                crate::edits::save(idx, &path, val);
            }
            let _ = std::fs::remove_file(&legacy);
            return Ok(edit);
        }
    }
    Ok(ClipEdit {
        segments: Vec::new(),
        mixer: MixerState::default(),
        format: Default::default(),
        look: Default::default(),
    })
}

#[cfg(not(target_os = "windows"))]
pub fn prepare_clip_audio(_path: String) -> Result<ClipAudio, String> {
    Err("El editor solo está disponible en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn keyframe_times(_path: String) -> Result<Vec<f64>, String> {
    Err("El editor solo está disponible en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn frame_times(_path: String) -> Result<Vec<f64>, String> {
    Err("El editor solo está disponible en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn clip_fps(_path: String) -> Result<u32, String> {
    Err("El editor solo está disponible en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn clip_dims(_path: String) -> Result<(u32, u32, u32, u32), String> {
    Err("El editor solo está disponible en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn export_clip<F: Fn(f32)>(
    _src: String,
    _dst: String,
    _edit: ClipEdit,
    _watermark: Option<String>,
    _bitrate: Option<u32>,
    _max_height: Option<u32>,
    _cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    _progress: F,
) -> Result<(), String> {
    Err("El editor solo está disponible en Windows".into())
}

// Marca de un export abortado a petición del usuario. Quien lo llama la usa para distinguirlo de un
// fallo real y no mostrar un error por algo que se pidió expresamente.
pub const CANCELLED: &str = "export-cancelled";

#[cfg(target_os = "windows")]
mod win {
    use std::sync::{Arc, Mutex, Once};
    use std::time::{Duration, Instant};

    use windows::core::{Interface, Result, GUID, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{E_FAIL, HMODULE};
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11Multithread, ID3D11Texture2D,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    };
    use windows::Win32::Graphics::Dxgi::IDXGISurface;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    use super::{ClipAudio, ClipEdit};

    const ALL_STREAMS: u32 = 0xFFFF_FFFE;
    const ENDOFSTREAM: u32 = 0x0000_0002;

    static MF_INIT: Once = Once::new();
    fn ensure_mf() {
        MF_INIT.call_once(|| unsafe {
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        });
    }

    // Ejecuta una operación de Media Foundation en su propio hilo con COM (MTA) y MF
    // inicializados. Los SourceReader/SinkWriter exigen ese contexto; sin él, fallan en
    // silencio en el hilo de comandos de Tauri.
    fn with_mf<T, F>(f: F) -> std::result::Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> std::result::Result<T, String> + Send + 'static,
    {
        std::thread::spawn(move || {
            unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
            ensure_mf();
            let r = f();
            unsafe { CoUninitialize(); }
            r
        })
        .join()
        .map_err(|_| "El hilo de Media Foundation terminó inesperadamente".to_string())?
    }

    pub fn prepare_clip_audio(path: String, audio_dir: String) -> std::result::Result<ClipAudio, String> {
        std::thread::spawn(move || {
            unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
            ensure_mf();
            let r = extract(&path, &audio_dir);
            unsafe { CoUninitialize(); }
            r
        })
        .join()
        .map_err(|_| "El hilo de extracción de audio terminó inesperadamente".to_string())?
    }

    fn extract(path: &str, audio_dir: &str) -> std::result::Result<ClipAudio, String> {
        let mf = |e| format!("{e:?}");
        let io = |e: std::io::Error| e.to_string();

        let audio_streams = count_audio_streams(path).map_err(mf)?;

        // Sin micro no hay pistas que separar: la pista única va embebida y la reproduce el propio
        // vídeo. Aun así se calcula su forma de onda (mezcla) para dibujarla en el editor.
        if audio_streams < 2 {
            if audio_streams == 1 {
                let (pcm, _sr, ch) = read_pcm(path, 0).map_err(mf)?;
                return Ok(ClipAudio {
                    mix_peaks: Some(peaks_from_pcm(&pcm, ch)),
                    ..Default::default()
                });
            }
            return Ok(ClipAudio::default());
        }

        let key = temp_key(path);
        let dir = std::path::Path::new(audio_dir);
        // `a2` versiona el formato de extracción: los WAV anteriores se generaban sin alinear el
        // hueco inicial de cada pista, así que se descartan (nombre nuevo) y se rehacen ya alineados.
        let sys = dir
            .join(format!("flashback_edit_{key}_a2_sys.wav"))
            .to_string_lossy()
            .into_owned();
        let mic = dir
            .join(format!("flashback_edit_{key}_a2_mic.wav"))
            .to_string_lossy()
            .into_owned();

        // Los clips son inmutables (edición no destructiva): si ya se separaron las pistas en
        // una apertura anterior, se reutilizan en vez de volver a volcar cientos de MB de WAV.
        // En ese caso los picos se sacan del WAV local (lectura barata) en vez de redecodificar.
        let ready = |p: &str| std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false);
        let (sys_peaks, mic_peaks) = if ready(&sys) && ready(&mic) {
            (peaks_from_wav(&sys), peaks_from_wav(&mic))
        } else {
            let (sys_pcm, sr, sc) = read_pcm(path, 1).map_err(mf)?;
            write_wav(&sys, &sys_pcm, sr, sc).map_err(io)?;
            let sp = peaks_from_pcm(&sys_pcm, sc);
            let (mic_pcm, mr, mc) = read_pcm(path, 0).map_err(mf)?;
            write_wav(&mic, &mic_pcm, mr, mc).map_err(io)?;
            let mp = peaks_from_pcm(&mic_pcm, mc);
            (Some(sp), Some(mp))
        };

        Ok(ClipAudio {
            system: Some(sys),
            mic: Some(mic),
            sys_peaks,
            mic_peaks,
            mix_peaks: None,
        })
    }

    // Clave estable por ruta completa: evita colisiones entre clips con el mismo nombre de
    // archivo en carpetas distintas (p. ej. el original y su `_edit.mp4`).
    fn temp_key(path: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    fn open_reader(path: &str) -> Result<IMFSourceReader> {
        let url = HSTRING::from(path);
        unsafe { MFCreateSourceReaderFromURL(&url, None) }
    }

    fn count_audio_streams(path: &str) -> Result<usize> {
        let reader = open_reader(path)?;
        let mut count = 0usize;
        let mut i = 0u32;
        while let Ok(mt) = unsafe { reader.GetNativeMediaType(i, 0) } {
            if unsafe { mt.GetGUID(&MF_MT_MAJOR_TYPE) }
                .map(|g| g == MFMediaType_Audio)
                .unwrap_or(false)
            {
                count += 1;
            }
            i += 1;
        }
        Ok(count)
    }

    // Índice del stream del `ordinal`-ésimo audio del fichero (0 = micro, 1 = sistema en los clips
    // de dos pistas). Los índices de stream no son correlativos con el orden de las pistas.
    fn audio_stream_at(reader: &IMFSourceReader, ordinal: usize) -> Option<u32> {
        let mut seen = 0usize;
        let mut i = 0u32;
        while let Ok(mt) = unsafe { reader.GetNativeMediaType(i, 0) } {
            let major = unsafe { mt.GetGUID(&MF_MT_MAJOR_TYPE) }.unwrap_or(GUID::zeroed());
            if major == MFMediaType_Audio {
                if seen == ordinal {
                    return Some(i);
                }
                seen += 1;
            }
            i += 1;
        }
        None
    }

    fn read_pcm(path: &str, ordinal: usize) -> Result<(Vec<u8>, u32, u16)> {
        let reader = open_reader(path)?;
        unsafe { reader.SetStreamSelection(ALL_STREAMS, false)? };

        let idx = audio_stream_at(&reader, ordinal).ok_or_else(|| {
            windows::core::Error::from(windows::core::HRESULT(0x80070002u32 as i32))
        })?;

        unsafe { reader.SetStreamSelection(idx, true)? };

        let pcm_type = unsafe { MFCreateMediaType()? };
        unsafe { pcm_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)? };
        unsafe { pcm_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)? };
        unsafe { reader.SetCurrentMediaType(idx, None, &pcm_type)? };

        let actual = unsafe { reader.GetCurrentMediaType(idx)? };
        let sr = unsafe { actual.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }.unwrap_or(48000);
        let ch = unsafe { actual.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }.unwrap_or(2) as u16;
        let frame_bytes = (ch.max(1) as usize) * 2;

        let mut pcm = Vec::new();
        loop {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            unsafe { reader.ReadSample(idx, 0, None, Some(&mut flags), None, Some(&mut sample))? };
            if flags & ENDOFSTREAM != 0 { break; }
            let Some(sample) = sample else { continue };
            // Alinear al origen de tiempo común (t=0). Si la pista arrancó tarde —el loopback de
            // sistema de WASAPI no entrega paquetes mientras no hay sonido—, su primer sample llega
            // con timestamp > 0 y el muxer dejó ese hueco en el MP4. Rellenamos con silencio hasta
            // su posición real para que sistema y micro queden sincronizados entre sí y con el
            // vídeo; concatenar sin más comprimía el hueco y desfasaba la pista varios segundos.
            let t = unsafe { sample.GetSampleTime() }.unwrap_or(0).max(0);
            let expected = (t as f64 / 10_000_000.0 * sr as f64).round() as usize * frame_bytes;
            if pcm.len() < expected {
                pcm.resize(expected, 0);
            }
            let buf = unsafe { sample.ConvertToContiguousBuffer()? };
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut cur = 0u32;
            unsafe { buf.Lock(&mut ptr, None, Some(&mut cur))? };
            if cur > 0 {
                let slice = unsafe { std::slice::from_raw_parts(ptr, cur as usize) };
                pcm.extend_from_slice(slice);
            }
            unsafe { buf.Unlock()? };
        }

        Ok((pcm, sr, ch))
    }

    fn write_wav(path: &str, pcm: &[u8], sample_rate: u32, channels: u16) -> std::io::Result<()> {
        use std::io::Write;
        let bits = 16u16;
        let byte_rate = sample_rate * channels as u32 * (bits as u32 / 8);
        let block_align = channels * (bits / 8);
        let data_len = pcm.len() as u32;
        let file_len = 36 + data_len;

        let mut f = std::fs::File::create(path)?;
        f.write_all(b"RIFF")?;
        f.write_all(&file_len.to_le_bytes())?;
        f.write_all(b"WAVE")?;
        f.write_all(b"fmt ")?;
        f.write_all(&16u32.to_le_bytes())?;
        f.write_all(&1u16.to_le_bytes())?;
        f.write_all(&channels.to_le_bytes())?;
        f.write_all(&sample_rate.to_le_bytes())?;
        f.write_all(&byte_rate.to_le_bytes())?;
        f.write_all(&block_align.to_le_bytes())?;
        f.write_all(&bits.to_le_bytes())?;
        f.write_all(b"data")?;
        f.write_all(&data_len.to_le_bytes())?;
        f.write_all(pcm)?;
        Ok(())
    }

    // Nº de cubos del envolvente: coincide con el ancho lógico que dibuja el editor. No hace falta
    // leer todas las muestras (millones en clips largos): se sondea a saltos dentro de cada cubo,
    // con coste fijo (~WAVE_BUCKETS × PEAK_PROBES) sea cual sea la duración.
    const WAVE_BUCKETS: usize = 1600;
    const PEAK_PROBES: usize = 96;

    fn peaks_from_pcm(pcm: &[u8], channels: u16) -> Vec<f32> {
        let ch = channels.max(1) as usize;
        let frames = pcm.len() / (ch * 2);
        let mut out = vec![0f32; WAVE_BUCKETS];
        if frames == 0 {
            return out;
        }
        let size = (frames / WAVE_BUCKETS).max(1);
        for (b, slot) in out.iter_mut().enumerate() {
            let start = b * size;
            if start >= frames {
                break;
            }
            let end = (start + size).min(frames);
            let span = end - start;
            let stride = if span > PEAK_PROBES { span / PEAK_PROBES } else { 1 };
            let mut peak = 0f32;
            let mut f = start;
            while f < end {
                let base = (f * ch) * 2;
                for c in 0..ch {
                    let idx = base + c * 2;
                    let v = i16::from_le_bytes([pcm[idx], pcm[idx + 1]]) as f32;
                    let a = if v < 0.0 { -v } else { v };
                    if a > peak {
                        peak = a;
                    }
                }
                f += stride;
            }
            *slot = peak / 32768.0;
        }
        out
    }

    // Picos desde un WAV PCM16 ya escrito por nosotros (cabecera fija de 44 bytes). Lee el archivo
    // local en vez de redecodificar el MP4 vía Media Foundation cuando las pistas ya están en caché.
    fn peaks_from_wav(path: &str) -> Option<Vec<f32>> {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() < 44 {
            return None;
        }
        let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
        Some(peaks_from_pcm(&bytes[44..], channels))
    }

    // Tiempos de presentación (ms) de TODOS los fotogramas de vídeo, ordenados. La captura WGC es de
    // framerate variable (frames solo cuando la pantalla cambia), así que para avanzar exactamente un
    // fotograma hay que conocer sus timestamps reales en vez de asumir un paso fijo. Mismo coste que
    // keyframe_times (una pasada de demux, sin decodificar).
    // Sonda del clip: un ÚNICO recorrido del MP4 del que salen los tiempos de fotograma, los
    // keyframes y si el orden de decodificación coincide con el de presentación. Sin
    // SetCurrentMediaType el lector entrega el H.264 tal cual, así que esto es solo demux (no se
    // decodifica nada) y cuesta lo que leer el fichero.
    struct ClipProbe {
        frames: Vec<f64>,
        keyframes: Vec<f64>,
        // Falso si hay B-frames (timestamps no monótonos en orden de decodificación). El camino
        // sin recodificar reordena paquetes, así que en ese caso no es seguro.
        in_order: bool,
    }

    // Los clips son inmutables (edición no destructiva), así que el resultado se cachea por
    // ruta + fecha + tamaño: antes se recorría el fichero entero tres veces (al abrir el editor,
    // al pedir keyframes y al exportar) para obtener siempre lo mismo.
    static PROBE_CACHE: Mutex<Vec<(String, (u64, u64), Arc<ClipProbe>)>> = Mutex::new(Vec::new());
    const PROBE_CACHE_MAX: usize = 8;

    fn file_stamp(path: &str) -> (u64, u64) {
        std::fs::metadata(path)
            .ok()
            .map(|m| {
                let t = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (t, m.len())
            })
            .unwrap_or((0, 0))
    }

    fn probe(path: &str) -> std::result::Result<Arc<ClipProbe>, String> {
        let stamp = file_stamp(path);
        if let Ok(cache) = PROBE_CACHE.lock() {
            if let Some((_, _, p)) = cache.iter().find(|(k, s, _)| k == path && *s == stamp) {
                return Ok(p.clone());
            }
        }
        let probe = Arc::new(scan_clip(path)?);
        if let Ok(mut cache) = PROBE_CACHE.lock() {
            cache.retain(|(k, _, _)| k != path);
            cache.push((path.to_string(), stamp, probe.clone()));
            if cache.len() > PROBE_CACHE_MAX {
                cache.remove(0);
            }
        }
        Ok(probe)
    }

    fn scan_clip(path: &str) -> std::result::Result<ClipProbe, String> {
        let mf = |e: windows::core::Error| format!("{e:?}");
        let reader = open_reader(path).map_err(mf)?;
        let idx = find_stream(&reader, MFMediaType_Video).map_err(mf)?;
        unsafe { reader.SetStreamSelection(ALL_STREAMS, false) }.map_err(mf)?;
        unsafe { reader.SetStreamSelection(idx, true) }.map_err(mf)?;

        let mut frames = Vec::new();
        let mut keyframes = Vec::new();
        let mut in_order = true;
        let mut last = i64::MIN;
        loop {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            unsafe { reader.ReadSample(idx, 0, None, Some(&mut flags), None, Some(&mut sample)) }
                .map_err(mf)?;
            if flags & ENDOFSTREAM != 0 { break; }
            let Some(sample) = sample else { continue };
            let t = unsafe { sample.GetSampleTime() }.unwrap_or(0);
            if t < last {
                in_order = false;
            }
            last = t;
            let ms = t as f64 / 10_000.0;
            if unsafe { sample.GetUINT32(&MFSampleExtension_CleanPoint) }.unwrap_or(0) != 0 {
                keyframes.push(ms);
            }
            frames.push(ms);
        }
        let asc = |a: &f64, b: &f64| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal);
        frames.sort_by(asc);
        keyframes.sort_by(asc);
        Ok(ClipProbe { frames, keyframes, in_order })
    }

    pub fn frame_times(path: String) -> std::result::Result<Vec<f64>, String> {
        with_mf(move || probe(&path).map(|p| p.frames.clone()))
    }

    pub fn keyframe_times(path: String) -> std::result::Result<Vec<f64>, String> {
        with_mf(move || probe(&path).map(|p| p.keyframes.clone()))
    }

    pub fn clip_fps(path: String) -> std::result::Result<u32, String> {
        with_mf(move || read_video_meta(&path).map(|m| m.fps).map_err(|e| format!("{e:?}")))
    }

    pub fn clip_dims(path: String) -> std::result::Result<(u32, u32, u32, u32), String> {
        with_mf(move || {
            read_video_meta(&path)
                .map(|m| (m.width, m.height, m.fps, m.bitrate))
                .map_err(|e| format!("{e:?}"))
        })
    }

    pub fn export_clip<F: Fn(f32) + Send + 'static>(
        src: String,
        dst: String,
        edit: ClipEdit,
        watermark: Option<String>,
        bitrate: Option<u32>,
        max_height: Option<u32>,
        cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        progress: F,
    ) -> std::result::Result<(), String> {
        std::thread::spawn(move || {
            unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
            ensure_mf();
            let r = do_export(&src, &dst, &edit, watermark.as_deref(), bitrate, max_height, cancel.as_deref(), &progress);
            // Un export abortado deja un MP4 a medias sin finalizar: se borra para que nadie lo
            // encuentre luego y lo tome por un clip válido.
            if r.as_ref().err().map(|e| e == super::CANCELLED).unwrap_or(false) {
                let _ = std::fs::remove_file(&dst);
            }
            unsafe { CoUninitialize(); }
            let r = r.and_then(|()| {
                let mov = std::path::Path::new(&dst)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("mov"));
                if mov {
                    crate::mp4mux::brand_quicktime(std::path::Path::new(&dst)).map_err(|e| e.to_string())?;
                }
                Ok(())
            });
            // El clip editado hereda el origen del original (juego/monitor) embebiéndolo igual que
            // en la captura, para que conserve su etiqueta en la biblioteca.
            if r.is_ok() {
                let source = crate::library::clip_source(std::path::Path::new(&src))
                    .unwrap_or_default();
                if !source.is_empty() {
                    let _ = crate::library::write_embedded_source(
                        std::path::Path::new(&dst),
                        &source,
                    );
                }
            }
            r
        })
        .join()
        .map_err(|_| "El hilo de exportación terminó inesperadamente".to_string())?
    }

    #[derive(Clone)]
    struct VideoMeta {
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
        // Solo el H.264 se puede copiar tal cual al MP4; VP9/AV1 (MKV, WebM) se recodifican.
        h264: bool,
    }

    // Fotogramas por segundo según la separación real entre fotogramas (mediana, para ignorar
    // huecos sueltos). Los tiempos pueden llegar en orden de decodificación si hay B-frames.
    fn fps_from_times(times: &mut [i64]) -> Option<u32> {
        if times.len() < 3 {
            return None;
        }
        times.sort_unstable();
        let mut deltas: Vec<i64> = times.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0).collect();
        if deltas.is_empty() {
            return None;
        }
        deltas.sort_unstable();
        let median = deltas[deltas.len() / 2];
        Some(((10_000_000.0 / median as f64).round() as u32).max(1))
    }

    // Tiempos de los primeros fotogramas: solo demux, sin decodificar ni recorrer el archivo.
    fn first_frame_times(reader: &IMFSourceReader, idx: u32) -> Vec<i64> {
        let mut times = Vec::new();
        unsafe {
            if reader.SetStreamSelection(ALL_STREAMS, false).is_err() || reader.SetStreamSelection(idx, true).is_err() {
                return times;
            }
        }
        while times.len() < 32 {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            if unsafe { reader.ReadSample(idx, 0, None, Some(&mut flags), None, Some(&mut sample)) }.is_err()
                || flags & ENDOFSTREAM != 0
            {
                break;
            }
            if let Some(t) = sample.and_then(|s| unsafe { s.GetSampleTime() }.ok()) {
                times.push(t);
            }
        }
        times
    }

    fn read_video_meta(path: &str) -> Result<VideoMeta> {
        let reader = open_reader(path)?;
        let v_idx = find_stream(&reader, MFMediaType_Video)?;
        let mt = unsafe { reader.GetCurrentMediaType(v_idx)? };
        let size = unsafe { mt.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(pack2(0, 0));
        let w = (size >> 32) as u32;
        let h = (size & 0xFFFFFFFF) as u32;
        let fps_packed = unsafe { mt.GetUINT64(&MF_MT_FRAME_RATE) }.unwrap_or(pack2(30, 1));
        let fps_n = (fps_packed >> 32) as u32;
        let fps_d = (fps_packed & 0xFFFFFFFF) as u32;
        // El MF_MT_FRAME_RATE de la fuente Matroska no es fiable (un MKV/WebM de 30 fps se anuncia
        // como 15,00002) y recodificar a esa cadencia tiraba la mitad de los fotogramas: manda la
        // separación medida, con el valor declarado como respaldo.
        let declared_fps = if fps_d == 0 { 30 } else { (fps_n as f64 / fps_d as f64).round() as u32 };
        let fps = fps_from_times(&mut first_frame_times(&reader, v_idx)).unwrap_or(declared_fps);
        // Bitrate objetivo al recodificar. MF casi nunca expone MF_MT_AVG_BITRATE en un MP4, así
        // que se mide del propio fichero (tamaño/duración) en vez de aplicar un suelo sintético
        // por resolución: aquel suelo imponía 15 Mbps en 1080p60 y 66 Mbps en 1440p144 aunque el
        // clip se hubiera grabado a mucho menos, y eso se paga en tiempo de codificación y en
        // tamaño del archivo exportado.
        let declared = unsafe { mt.GetUINT32(&MF_MT_AVG_BITRATE) }.unwrap_or(0);
        let measured = measured_bitrate(path).unwrap_or(0);
        let bitrate = declared.max(measured);
        let bitrate = if bitrate == 0 {
            ((w as u64 * h as u64 * fps.max(1) as u64) / 16).clamp(2_000_000, 40_000_000) as u32
        } else {
            bitrate
        };

        Ok(VideoMeta {
            width: w,
            height: h,
            fps: fps.max(1),
            bitrate: bitrate.clamp(500_000, 120_000_000),
            h264: unsafe { mt.GetGUID(&MF_MT_SUBTYPE) }.is_ok_and(|g| g == MFVideoFormat_H264),
        })
    }

    // Bitrate real del vídeo: bits del fichero entre su duración, descontando un margen fijo para
    // el audio y las cabeceras del contenedor. La duración sale del `mvhd` del MP4 (lectura de unos
    // pocos bytes), no de abrir un pipeline de Media Foundation.
    fn measured_bitrate(path: &str) -> Option<u32> {
        let p = std::path::Path::new(path);
        let len = std::fs::metadata(p).ok()?.len();
        let secs = crate::library::clip_duration_secs(p)?;
        if secs <= 0.1 {
            return None;
        }
        let total = len as f64 * 8.0 / secs;
        Some((total as u64).saturating_sub(320_000).clamp(500_000, 120_000_000) as u32)
    }

    fn find_stream(reader: &IMFSourceReader, kind: GUID) -> Result<u32> {
        let mut i = 0u32;
        while let Ok(mt) = unsafe { reader.GetNativeMediaType(i, 0) } {
            let major = unsafe { mt.GetGUID(&MF_MT_MAJOR_TYPE) }
                .unwrap_or(GUID::zeroed());
            if major == kind {
                return Ok(i);
            }
            i += 1;
        }
        Err(windows::core::Error::from(windows::core::HRESULT(0x80070002u32 as i32)))
    }

    // Hornea la marca de agua sobre el frame. En GPU se compone con Direct2D directamente sobre la
    // textura (sin bajar el frame a memoria de sistema); si el frame no viene en GPU —o la textura
    // no admite ser destino de dibujo— cae al blend por CPU sobre el BGRA. Best-effort: si nada
    // funciona, el frame se escribe sin marca en vez de romper el export. El camino CPU usa
    // IMF2DBuffer para el stride real (con signo: negativo si el buffer es bottom-up); si no lo
    // soporta, asume top-down (ancho*4).
    fn blend_watermark(
        sample: &IMFSample,
        logo: &crate::watermark::Logo,
        gpu_logo: Option<&crate::watermark::GpuLogo>,
        w: u32,
        h: u32,
    ) {
        unsafe {
            let Ok(buf) = sample.GetBufferByIndex(0) else { return };
            if let Some(gpu) = gpu_logo {
                if let Ok(dxgi) = buf.cast::<IMFDXGIBuffer>() {
                    // Direct2D solo puede dibujar sobre la subtextura 0 de un array.
                    if dxgi.GetSubresourceIndex().unwrap_or(1) == 0 {
                        let mut tex: Option<ID3D11Texture2D> = None;
                        let got = dxgi
                            .GetResource(
                                &ID3D11Texture2D::IID,
                                &mut tex as *mut Option<ID3D11Texture2D> as *mut *mut std::ffi::c_void,
                            )
                            .is_ok();
                        if got {
                            if let Some(surface) = tex.and_then(|t| t.cast::<IDXGISurface>().ok()) {
                                if gpu.blend(&surface, w, h).is_ok() {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            if let Ok(b2) = buf.cast::<IMF2DBuffer>() {
                let mut scan0: *mut u8 = std::ptr::null_mut();
                let mut pitch: i32 = 0;
                if b2.Lock2D(&mut scan0, &mut pitch).is_ok() {
                    if !scan0.is_null() {
                        logo.blend(scan0, pitch as isize, w, h);
                    }
                    let _ = b2.Unlock2D();
                    return;
                }
            }
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut cur = 0u32;
            if buf.Lock(&mut ptr, None, Some(&mut cur)).is_ok() {
                if !ptr.is_null() {
                    logo.blend(ptr, (w as isize) * 4, w, h);
                }
                let _ = buf.Unlock();
            }
        }
    }

    // Device D3D11 propio del export, deliberadamente separado del de la captura: las texturas no
    // cruzan devices y, sobre todo, el camino de captura es sagrado y no debe cargar con el trabajo
    // del export. Tampoco se le sube la prioridad de GPU (la captura sí lo hace) para no competir
    // con una grabación en curso.
    pub(crate) struct Gpu {
        pub(crate) device: ID3D11Device,
        pub(crate) manager: IMFDXGIDeviceManager,
    }

    pub(crate) fn create_gpu() -> Option<Gpu> {
        // Válvula de escape para comparar contra el camino por CPU sin recompilar.
        if std::env::var_os("FLASHBACK_EXPORT_CPU").is_some() {
            return None;
        }
        unsafe {
            let mut device: Option<ID3D11Device> = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )
            .ok()?;
            let device = device?;
            // Media Foundation toca el device desde sus propios hilos (decodificador, procesador
            // de vídeo y encoder corren cada uno por su lado).
            if let Ok(ctx) = device.GetImmediateContext() {
                if let Ok(mt) = ctx.cast::<ID3D11Multithread>() {
                    let _ = mt.SetMultithreadProtected(true);
                }
            }
            let mut token = 0u32;
            let mut manager: Option<IMFDXGIDeviceManager> = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager).ok()?;
            let manager = manager?;
            manager.ResetDevice(&device, token).ok()?;
            Some(Gpu { device, manager })
        }
    }

    // SinkWriter a MP4 con el índice (`moov`) delante del `mdat`: sin eso el reproductor tiene que
    // escanear el fichero entero al abrirlo, y un clip recién exportado tardaba segundos en pintar
    // su primer frame en el editor. Mismo criterio que el muxer del replay.
    fn create_sink(
        dst: &str,
        manager: Option<&IMFDXGIDeviceManager>,
        passthrough: bool,
    ) -> Result<IMFSinkWriter> {
        let url = HSTRING::from(dst);
        let byte_stream = unsafe {
            MFCreateFile(
                MF_ACCESSMODE_READWRITE,
                MF_OPENMODE_DELETE_IF_EXIST,
                MF_FILEFLAGS_NONE,
                &url,
            )?
        };
        if let Ok(bs) = byte_stream.cast::<IMFAttributes>() {
            unsafe {
                let _ = bs.SetUINT32(&MF_MPEG4SINK_MOOV_BEFORE_MDAT, 1);
            }
        }
        let attrs = unsafe {
            let mut a: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut a, 4)?;
            let a = a.ok_or_else(|| windows::core::Error::from(E_FAIL))?;
            a.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            // Sin throttling el SinkWriter acepta samples sin límite. Eso solo es seguro copiando
            // paquetes ya codificados: al recodificar, si el encoder se queda atrás, la cola crece
            // sin freno —y con texturas de GPU el lector se queda sin pool y ReadSample se bloquea
            // para siempre—. Con throttling, WriteSample hace contrapresión y los buffers vuelven.
            if passthrough {
                a.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            }
            // Sin URL no se infiere el contenedor: hay que decirlo explícitamente.
            a.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
            if let Some(m) = manager {
                let unk: windows::core::IUnknown = m.cast()?;
                a.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &unk)?;
            }
            a
        };
        unsafe { MFCreateSinkWriterFromURL(PCWSTR::null(), &byte_stream, &attrs) }
    }

    // El progreso viaja al frontend por IPC. Emitirlo cada pocos fotogramas llenaba el canal de
    // eventos sin que se notara en pantalla, así que se limita por tiempo.
    struct Progress<'a> {
        emit: &'a dyn Fn(f32),
        last: Instant,
    }

    impl<'a> Progress<'a> {
        fn new(emit: &'a dyn Fn(f32)) -> Self {
            Progress { emit, last: Instant::now() - Duration::from_secs(1) }
        }
        fn set(&mut self, v: f32) {
            if self.last.elapsed() >= Duration::from_millis(80) {
                (self.emit)(v.clamp(0.0, 0.97));
                self.last = Instant::now();
            }
        }
        fn force(&mut self, v: f32) {
            (self.emit)(v);
            self.last = Instant::now();
        }
    }

    // Qué hay que hacer con el audio del clip.
    enum AudioPlan {
        None,
        // Pista única con el fader intacto: en passthrough se copia el AAC tal cual.
        Copy(u32),
        // Pista única con ganancia: se decodifica a PCM, se escala y se recodifica.
        Gain(u32, f32),
        // Sistema + micro: hay que hornear la mezcla en una sola pista (un reproductor normal solo
        // suena la primera), así que el audio siempre se recodifica.
        Remix,
    }

    fn audio_plan(src: &str, edit: &ClipEdit) -> Result<AudioPlan> {
        let reader = open_reader(src)?;
        // Dos pistas = sistema + micro: hay mezcla que hornear.
        if audio_stream_at(&reader, 1).is_some() {
            return Ok(AudioPlan::Remix);
        }
        let Some(idx) = audio_stream_at(&reader, 0) else {
            return Ok(AudioPlan::None);
        };
        let gain = if edit.mixer.sys_muted { 0.0 } else { edit.mixer.sys_vol };
        Ok(if (gain - 1.0).abs() < 1e-3 {
            AudioPlan::Copy(idx)
        } else {
            AudioPlan::Gain(idx, gain)
        })
    }

    // La mezcla de audio corre en su propio hilo mientras el vídeo se codifica: antes se hacía
    // después, en serie, y decodificar dos pistas de varios minutos costaba lo suyo. `take` espera
    // el resultado; si el hilo ya se consumió (reintento tras un passthrough fallido), la rehace.
    struct Remix {
        src: String,
        edit: ClipEdit,
        job: Option<std::thread::JoinHandle<std::result::Result<(Vec<i16>, u32), String>>>,
    }

    impl Remix {
        fn spawn(src: &str, edit: &ClipEdit) -> Remix {
            let (s, e) = (src.to_string(), edit.clone());
            let job = std::thread::spawn(move || {
                unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
                ensure_mf();
                let r = build_remixed_pcm(&s, &e).map_err(|e| format!("{e:?}"));
                unsafe { CoUninitialize(); }
                r
            });
            Remix { src: src.to_string(), edit: edit.clone(), job: Some(job) }
        }

        fn take(&mut self) -> std::result::Result<(Vec<i16>, u32), String> {
            match self.job.take() {
                Some(job) => job
                    .join()
                    .map_err(|_| "El hilo de mezcla de audio terminó inesperadamente".to_string())?,
                None => build_remixed_pcm(&self.src, &self.edit).map_err(|e| format!("{e:?}")),
            }
        }
    }

    // Un MP4 solo puede abrir un tramo en un IDR, así que el camino sin recodificar exige que cada
    // tramo arranque justo en un keyframe. El final puede caer donde sea: los fotogramas que quedan
    // detrás se descartan y nadie los referencia. Recortar solo el final —el caso más habitual—
    // siempre cumple, porque el tramo empieza en 0 y el primer frame es IDR por definición.
    fn cuts_on_keyframes(edit: &ClipEdit, probe: &ClipProbe, fps: u32) -> bool {
        if probe.keyframes.is_empty() {
            return false;
        }
        let tol = 500.0 / fps.max(1) as f64;
        edit.segments.iter().all(|seg| {
            seg.end_ms <= seg.start_ms
                || probe.keyframes.iter().any(|k| (k - seg.start_ms).abs() <= tol)
        })
    }

    // Mapa origen→salida de cada tramo: (inicio, fin, desplazamiento). Los tramos se concatenan sin
    // huecos, así que vídeo y audio comparten exactamente este mapeo y el sync se mantiene.
    fn segment_ranges(edit: &ClipEdit) -> Vec<(i64, i64, i64)> {
        let mut kept = 0i64;
        edit.segments
            .iter()
            .map(|seg| {
                let s = (seg.start_ms * 10_000.0) as i64;
                let e = (seg.end_ms * 10_000.0) as i64;
                let offset = s - kept;
                kept += e - s;
                (s, e, offset)
            })
            .collect()
    }

    fn kept_hns(edit: &ClipEdit) -> i64 {
        edit.segments
            .iter()
            .map(|s| ((s.end_ms - s.start_ms).max(0.0) * 10_000.0) as i64)
            .sum::<i64>()
            .max(1)
    }

    #[allow(clippy::too_many_arguments)]
    fn do_export(
        src: &str,
        dst: &str,
        edit: &ClipEdit,
        watermark: Option<&str>,
        bitrate: Option<u32>,
        max_height: Option<u32>,
        cancel: Option<&std::sync::atomic::AtomicBool>,
        progress: &dyn Fn(f32),
    ) -> std::result::Result<(), String> {
        let mf = |e: windows::core::Error| format!("{e:?}");

        if edit.segments.is_empty() {
            return Err("No hay segmentos para exportar".into());
        }
        let started = Instant::now();
        let mut meta = read_video_meta(src).map_err(mf)?;
        // Compartir con un tamaño objetivo impone su propio bitrate; el export normal pasa None y
        // conserva el que se dedujo del origen.
        if let Some(bps) = bitrate {
            meta.bitrate = bps.max(100_000);
        }
        // Sin preset, el bitrate vertical se escala por píxeles de salida: el recorte amplía una
        // ventana del origen y copiar el bitrate de 1920x1080 no siempre encaja.
        let vertical = matches!(edit.format, crate::reframe::OutputFormat::Vertical { .. });
        if vertical && bitrate.is_none() {
            let (vw, vh) = crate::reframe::vertical_size(max_height);
            meta.bitrate = crate::reframe::vertical_bitrate(meta.bitrate, meta.width, meta.height, vw, vh);
        }
        let clip = probe(src)?;
        let probed = started.elapsed();

        let plan = audio_plan(src, edit).map_err(mf)?;
        let mut remix = matches!(plan, AudioPlan::Remix).then(|| Remix::spawn(src, edit));
        let mut progress = Progress::new(progress);

        let scaled = max_height.map(|mh| mh < meta.height).unwrap_or(false);
        // El vídeo puede copiarse tal cual cuando no hay nada que repintar ni reescalar y los
        // cortes caen en keyframe. El audio no condiciona la decisión: recodificarlo cuesta una
        // fracción de lo que cuesta el vídeo, así que cabe dentro del camino sin recodificar.
        let graded = !edit.look.clamped().is_neutral();
        let can_pass = meta.h264
            && watermark.is_none()
            && !vertical
            && !graded
            && bitrate.is_none()
            && !scaled
            && clip.in_order
            && cuts_on_keyframes(edit, &clip, meta.fps);

        if can_pass {
            let r = passthrough_export(
                src, dst, edit, &meta, &plan, remix.as_mut(), cancel, &mut progress,
            );
            match r {
                Ok(()) => {
                    progress.force(1.0);
                    eprintln!(
                        "export: copia sin recodificar · {}x{} · sonda {:?} · total {:?}",
                        meta.width, meta.height, probed, started.elapsed()
                    );
                    return Ok(());
                }
                Err(e) if e == super::CANCELLED => return Err(e),
                Err(e) => eprintln!("export: la copia sin recodificar falló ({e}); se recodifica"),
            }
        }

        let gpu = create_gpu();
        // La captura ya exige D3D11 por hardware, así que no hay camino por CPU para el vertical ni
        // para los ajustes de imagen: los dos se componen con Direct2D.
        if vertical && gpu.is_none() {
            return Err("El formato vertical necesita una GPU compatible con DirectX 11".into());
        }
        if graded && gpu.is_none() {
            return Err("Los ajustes de imagen necesitan una GPU compatible con DirectX 11".into());
        }
        let r = reencode_export(
            src, dst, edit, &meta, &plan, remix.as_mut(), gpu.as_ref(), watermark, max_height,
            cancel, &mut progress,
        );
        if r.is_ok() {
            progress.force(1.0);
            eprintln!(
                "export: recodificado en {} · {}x{} {} kbps · sonda {:?} · total {:?}",
                if gpu.is_some() { "GPU" } else { "CPU" },
                meta.width,
                meta.height,
                meta.bitrate / 1000,
                probed,
                started.elapsed()
            );
        }
        r
    }

    // Exporta sin recodificar: los paquetes H.264 del original se copian al MP4 de salida tal cual,
    // retemporizados para concatenar los tramos sin huecos. Es el camino natural de un montaje que
    // solo corta, y cuesta lo que escribir el fichero.
    #[allow(clippy::too_many_arguments)]
    fn passthrough_export(
        src: &str,
        dst: &str,
        edit: &ClipEdit,
        meta: &VideoMeta,
        plan: &AudioPlan,
        remix: Option<&mut Remix>,
        cancel: Option<&std::sync::atomic::AtomicBool>,
        progress: &mut Progress,
    ) -> std::result::Result<(), String> {
        let mf = |e: windows::core::Error| format!("{e:?}");
        let aborted = || {
            cancel
                .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(false)
        };

        // Lector de vídeo sin SetCurrentMediaType: entrega el H.264 comprimido, sin decodificador.
        let v_reader = open_reader(src).map_err(mf)?;
        let v_idx = find_stream(&v_reader, MFMediaType_Video).map_err(mf)?;
        unsafe { v_reader.SetStreamSelection(ALL_STREAMS, false).map_err(mf)? };
        unsafe { v_reader.SetStreamSelection(v_idx, true).map_err(mf)? };

        // Copia del tipo nativo: arrastra el SPS/PPS (MF_MT_MPEG_SEQUENCE_HEADER) con el que el
        // sink MP4 escribe el `avcC`. Entrada y salida son el mismo tipo; eso es el passthrough.
        let v_type = unsafe { MFCreateMediaType().map_err(mf)? };
        unsafe {
            let native = v_reader.GetNativeMediaType(v_idx, 0).map_err(mf)?;
            native.CopyAllItems(&v_type).map_err(mf)?;
            let _ = v_type.SetUINT32(&MF_MT_AVG_BITRATE, meta.bitrate);
        }

        let sink = create_sink(dst, None, true).map_err(mf)?;
        let v_stream = unsafe { sink.AddStream(&v_type).map_err(mf)? };
        unsafe { sink.SetInputMediaType(v_stream, &v_type, None).map_err(mf)? };

        // El audio va en su propio lector: el del vídeo se reposiciona en cada tramo y un
        // SetCurrentPosition mueve todos los streams seleccionados a la vez.
        let a_reader = open_reader(src).map_err(mf)?;
        let mut audio = build_audio_writer(&sink, src, edit, plan, &a_reader, remix, true)?;

        unsafe { sink.BeginWriting().map_err(mf)? };

        let total = kept_hns(edit) as f32;
        let frame_dur = (10_000_000 / meta.fps.max(1) as i64).max(1);
        let tol = frame_dur / 2;
        let mut kept_before: i64 = 0;
        for seg in &edit.segments {
            let start_hns = (seg.start_ms * 10_000.0) as i64;
            let end_hns = (seg.end_ms * 10_000.0) as i64;
            if end_hns <= start_hns {
                continue;
            }
            // Se busca desde algo ANTES del corte: MF aterriza en el keyframe igual o anterior al
            // tiempo pedido, y un redondeo de microsegundos al pasar de ms a hns podría dejarlo en
            // el keyframe siguiente. Los paquetes sobrantes se descartan; son demux, no decodificación.
            let pos = PROPVARIANT::from((start_hns - tol).max(0));
            unsafe { v_reader.SetCurrentPosition(&GUID::zeroed(), &pos).map_err(mf)? };

            let mut opened = false;
            loop {
                if aborted() {
                    return Err(super::CANCELLED.into());
                }
                let mut flags = 0u32;
                let mut sample: Option<IMFSample> = None;
                unsafe {
                    v_reader
                        .ReadSample(v_idx, 0, None, Some(&mut flags), None, Some(&mut sample))
                        .map_err(mf)?;
                }
                if flags & ENDOFSTREAM != 0 { break; }
                let Some(sample) = sample else { continue };
                let t = unsafe { sample.GetSampleTime().map_err(mf)? };
                if t + tol < start_hns { continue; }
                // Red de seguridad del contenedor: si el primer paquete del tramo no fuera un IDR,
                // el MP4 saldría indecodificable desde ese punto. Se aborta y el llamador recodifica,
                // que siempre da un resultado correcto.
                if !opened {
                    if unsafe { sample.GetUINT32(&MFSampleExtension_CleanPoint) }.unwrap_or(0) == 0 {
                        return Err("el tramo no arranca en un keyframe".into());
                    }
                    opened = true;
                }
                if t >= end_hns { break; }
                let out_t = (t - start_hns) + kept_before;
                let dur = unsafe { sample.GetSampleDuration() }.unwrap_or(frame_dur).max(1);
                unsafe {
                    sample.SetSampleTime(out_t).map_err(mf)?;
                    sample.SetSampleDuration(dur).map_err(mf)?;
                    sink.WriteSample(v_stream, &sample).map_err(mf)?;
                }
                if let Some(a) = audio.as_mut() {
                    a.advance(&sink, out_t, cancel)?;
                }
                progress.set(out_t as f32 / total);
            }
            kept_before += end_hns - start_hns;
        }

        if aborted() {
            return Err(super::CANCELLED.into());
        }
        if let Some(a) = audio.as_mut() {
            a.advance(&sink, i64::MAX, cancel)?;
        }

        progress.force(0.99);
        unsafe { sink.Finalize().map_err(mf)? };
        Ok(())
    }

    // Exporta recodificando con un ÚNICO encoder por hardware: así el clip de salida tiene un solo
    // SPS/PPS coherente con su avcC. Una versión anterior copiaba la mayor parte y recodificaba
    // solo el GOP del borde, mezclando dos SPS distintos bajo un único avcC, y el decodificador
    // estricto del editor (WebView2) mostraba el primer frame en negro. El recorte es exacto al
    // frame: se decodifica desde el keyframe anterior al corte pero solo se escribe a partir de él.
    #[allow(clippy::too_many_arguments)]
    fn reencode_export(
        src: &str,
        dst: &str,
        edit: &ClipEdit,
        meta: &VideoMeta,
        plan: &AudioPlan,
        remix: Option<&mut Remix>,
        gpu: Option<&Gpu>,
        watermark: Option<&str>,
        max_height: Option<u32>,
        cancel: Option<&std::sync::atomic::AtomicBool>,
        progress: &mut Progress,
    ) -> std::result::Result<(), String> {
        let mf = |e: windows::core::Error| format!("{e:?}");
        let aborted = || {
            cancel
                .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(false)
        };

        let clip = probe(src)?;
        let keyframes = &clip.keyframes;

        // Lector con procesado de vídeo avanzado (conversión de color y reescalado sin insertar
        // ningún MFT a mano) y, si hay GPU, con el device manager: eso enciende la decodificación
        // por DXVA y deja la conversión en la tarjeta en vez de en la CPU.
        let v_reader = {
            let attrs = unsafe {
                let mut a: Option<IMFAttributes> = None;
                MFCreateAttributes(&mut a, 2).map_err(mf)?;
                let a = a.ok_or_else(|| "Atributos nulos".to_string())?;
                a.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1).map_err(mf)?;
                if let Some(g) = gpu {
                    let unk: windows::core::IUnknown = g.manager.cast().map_err(mf)?;
                    a.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &unk).map_err(mf)?;
                }
                a
            };
            let url = HSTRING::from(src);
            unsafe { MFCreateSourceReaderFromURL(&url, &attrs).map_err(mf)? }
        };
        let v_idx = find_stream(&v_reader, MFMediaType_Video).map_err(mf)?;
        unsafe { v_reader.SetStreamSelection(ALL_STREAMS, false).map_err(mf)? };
        unsafe { v_reader.SetStreamSelection(v_idx, true).map_err(mf)? };

        // Reescalado opcional (presets de tamaño de compartir): lo hace el propio procesador del
        // lector. Si rechaza el tamaño pedido se reintenta con el nativo en vez de fallar.
        let vertical = match edit.format {
            crate::reframe::OutputFormat::Vertical { fill, zoom } => Some(crate::reframe::zoom_of(fill, zoom)),
            crate::reframe::OutputFormat::Horizontal => None,
        };
        let look = edit.look.clamped();
        let graded = !look.is_neutral();
        let frame_dur = (10_000_000 / meta.fps.max(1) as i64).max(1);
        // En vertical el origen se decodifica a tamaño nativo: el recorte amplía una ventana y
        // reescalar antes solo perdería detalle. El preset se aplica al tamaño de salida.
        let (req_w, req_h) = match max_height {
            Some(mh) if mh < meta.height && vertical.is_none() => {
                let w = (meta.width as u64 * mh as u64 / meta.height.max(1) as u64) as u32;
                ((w + 1) & !1, (mh + 1) & !1)
            }
            _ => (meta.width, meta.height),
        };
        let (v_in, on_gpu) = negotiate_video(
            &v_reader,
            v_idx,
            gpu.is_some(),
            watermark.is_some() || vertical.is_some() || graded,
            req_w,
            req_h,
            meta.width,
            meta.height,
        )?;
        unsafe { v_in.SetUINT64(&MF_MT_FRAME_RATE, pack2(meta.fps, 1)).map_err(mf)? };
        // Tamaño REAL concedido por el lector: manda sobre el pedido. El resto del pipeline (salida
        // del encoder y marca de agua) tiene que cuadrar con lo que de verdad entrega el decoder.
        let out_size =
            unsafe { v_in.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(pack2(meta.width, meta.height));
        let out_w = (out_size >> 32) as u32;
        let out_h = (out_size & 0xFFFF_FFFF) as u32;
        // Sin vertical, el compositor solo hace falta para los ajustes de imagen y deja el tamaño
        // tal cual (el reescalado de un preset ya lo hizo el lector).
        let layout = match vertical {
            Some(zoom) => Some((crate::reframe::win::Layout::Vertical(zoom), crate::reframe::vertical_size(max_height))),
            None if graded => Some((crate::reframe::win::Layout::Full, (out_w, out_h))),
            None => None,
        };
        let reframer = match (layout, gpu) {
            (Some((layout, (vw, vh))), Some(g)) => Some(
                crate::reframe::win::Reframer::new(&g.device, &g.manager, out_w, out_h, vw, vh, meta.fps, layout, &look)
                    .map_err(mf)?,
            ),
            _ => None,
        };
        // Tamaño de lo que llega al encoder: el vertical si hay Reframer, el decodificado si no.
        let (enc_w, enc_h) = reframer.as_ref().map_or((out_w, out_h), |r| r.out_size());
        let gpu_frames = on_gpu || reframer.is_some();

        // Marca de agua: se rasteriza una vez al tamaño de salida. Best-effort: si falla, se exporta
        // sin marca (no se rompe el export). El blend por frame va más abajo, antes de WriteSample.
        let logo = watermark.and_then(|c| {
            match crate::watermark::Logo::rasterize(enc_w, enc_h, crate::watermark::Corner::parse(c)) {
                Ok(l) => Some(l),
                Err(e) => {
                    eprintln!("watermark: rasterización falló, exporto sin marca: {e:?}");
                    None
                }
            }
        });
        let gpu_logo = match (&logo, gpu) {
            (Some(l), Some(g)) if gpu_frames => l.to_gpu(&g.device).ok(),
            _ => None,
        };

        let sink = create_sink(dst, gpu.filter(|_| gpu_frames).map(|g| &g.manager), false).map_err(mf)?;

        let v_out = unsafe { MFCreateMediaType().map_err(mf)? };
        unsafe {
            v_out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(mf)?;
            v_out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(mf)?;
            v_out.SetUINT64(&MF_MT_FRAME_SIZE, pack2(enc_w, enc_h)).map_err(mf)?;
            v_out.SetUINT64(&MF_MT_FRAME_RATE, pack2(meta.fps, 1)).map_err(mf)?;
            v_out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(mf)?;
            v_out.SetUINT32(&MF_MT_AVG_BITRATE, meta.bitrate).map_err(mf)?;
        }
        let v_stream = unsafe { sink.AddStream(&v_out).map_err(mf)? };
        // Entrada del encoder = tipo de salida REAL del decodificador: evita desajustes de geometría.
        let enc_in = reframer.as_ref().map_or(&v_in, |r| r.out_type());
        unsafe { sink.SetInputMediaType(v_stream, enc_in, None).map_err(mf)? };

        let a_reader = open_reader(src).map_err(mf)?;
        // El audio se recodifica siempre por este camino: cuesta una fracción de lo que cuesta el
        // vídeo y evita mezclar en el mismo sink un stream copiado con otro codificado.
        let mut audio = build_audio_writer(&sink, src, edit, plan, &a_reader, remix, false)?;

        unsafe { sink.BeginWriting().map_err(mf)? };

        let total = kept_hns(edit) as f32;
        let mut write = |sample: IMFSample, out_t: i64| -> std::result::Result<(), String> {
            if let Some(logo) = &logo {
                blend_watermark(&sample, logo, gpu_logo.as_ref(), enc_w, enc_h);
            }
            unsafe {
                sample.SetSampleTime(out_t).map_err(mf)?;
                sample.SetSampleDuration(frame_dur).map_err(mf)?;
                sink.WriteSample(v_stream, &sample).map_err(mf)?;
            }
            if let Some(a) = audio.as_mut() {
                a.advance(&sink, out_t, cancel)?;
            }
            progress.set(out_t as f32 / total);
            Ok(())
        };
        let mut kept_before: i64 = 0;
        for seg in &edit.segments {
            let start_hns = (seg.start_ms * 10_000.0) as i64;
            let end_hns = (seg.end_ms * 10_000.0) as i64;
            if end_hns <= start_hns {
                continue;
            }
            // Posicionar el lector en el keyframe anterior al inicio del tramo: decodifica desde ahí
            // (necesario para las referencias) pero solo se escriben los frames a partir del corte.
            let kf = keyframes.iter().rev().find(|k| **k <= seg.start_ms).copied().unwrap_or(0.0);
            let pos = PROPVARIANT::from((kf * 10_000.0) as i64);
            unsafe { v_reader.SetCurrentPosition(&GUID::zeroed(), &pos).map_err(mf)? };

            loop {
                if aborted() {
                    return Err(super::CANCELLED.into());
                }
                let mut flags = 0u32;
                let mut sample: Option<IMFSample> = None;
                unsafe {
                    v_reader
                        .ReadSample(v_idx, 0, None, Some(&mut flags), None, Some(&mut sample))
                        .map_err(mf)?;
                }
                if flags & ENDOFSTREAM != 0 { break; }
                let Some(sample) = sample else { continue };
                let t = unsafe { sample.GetSampleTime().map_err(mf)? };
                if t < start_hns { continue; }
                if t >= end_hns { break; }
                // Re-tiempo al hueco eliminado, idéntico al del audio: los tramos se concatenan sin
                // huecos en la salida (mismo mapeo origen→salida que el audio para mantener el sync).
                let out_t = (t - start_hns) + kept_before;
                let (cx, cy) = (seg.crop_x.unwrap_or(0.5), seg.crop_y.unwrap_or(0.5));
                match &reframer {
                    Some(r) => write(r.process(&sample, cx, cy).map_err(mf)?, out_t)?,
                    None => write(sample, out_t)?,
                }
            }
            kept_before += end_hns - start_hns;
        }

        if aborted() {
            return Err(super::CANCELLED.into());
        }
        if let Some(a) = audio.as_mut() {
            a.advance(&sink, i64::MAX, cancel)?;
        }

        progress.force(0.99);
        unsafe { sink.Finalize().map_err(mf)? };
        Ok(())
    }

    // Formato que se le pide al decodificador, y si los frames llegan como textura de GPU.
    //  - NV12 en GPU: es justo lo que consume el encoder, así que hay UNA conversión y ocurre en la
    //    tarjeta; la textura no baja nunca a memoria de sistema.
    //  - ARGB32 en GPU: alternativa cuando NV12 no cuadra sin relleno, y obligatoria con marca de
    //    agua porque el logo se compone en BGRA.
    //  - RGB32 por CPU como último recurso. Alimentar el NV12 del decodificador por memoria de
    //    sistema dejaba una franja verde arriba (el plano luma se alinea a múltiplo de 16 y el croma
    //    se leía desfasado); RGB32 no tiene planos de croma y el desajuste desaparece.
    #[allow(clippy::too_many_arguments)]
    fn negotiate_video(
        reader: &IMFSourceReader,
        idx: u32,
        gpu: bool,
        watermark: bool,
        req_w: u32,
        req_h: u32,
        native_w: u32,
        native_h: u32,
    ) -> std::result::Result<(IMFMediaType, bool), String> {
        let mf = |e: windows::core::Error| format!("{e:?}");

        // Pone el tipo y devuelve lo que el lector entrega DE VERDAD. El tamaño real solo se
        // conoce después de pasar un fotograma: el lector no cierra la negociación hasta que el
        // pipeline corre, y a 1080p acaba concediendo NV12 de 1920x1088 (plano luma alineado a
        // múltiplo de 16) aunque al pedirlo aceptara 1080. Preguntarle antes de tiempo devolvía el
        // tamaño pedido y esas ocho filas de relleno acababan dentro del vídeo exportado.
        let settle = |subtype: &GUID,
                      w: u32,
                      h: u32|
         -> std::result::Result<Option<(IMFMediaType, bool, u32, u32)>, String> {
            unsafe {
                let Ok(t) = MFCreateMediaType() else { return Ok(None) };
                if t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).is_err()
                    || t.SetGUID(&MF_MT_SUBTYPE, subtype).is_err()
                    || t.SetUINT64(&MF_MT_FRAME_SIZE, pack2(w, h)).is_err()
                    || reader.SetCurrentMediaType(idx, None, &t).is_err()
                {
                    return Ok(None);
                }
            }
            let on_gpu = delivers_gpu_textures(reader, idx)?;
            // Copia del tipo negociado: el objeto del lector no se toca.
            let copy = unsafe { MFCreateMediaType().map_err(mf)? };
            unsafe {
                reader.GetCurrentMediaType(idx).map_err(mf)?.CopyAllItems(&copy).map_err(mf)?;
            }
            let size = unsafe { copy.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(pack2(w, h));
            Ok(Some((copy, on_gpu, (size >> 32) as u32, (size & 0xFFFF_FFFF) as u32)))
        };

        let sizes = [(req_w, req_h), (native_w, native_h)];
        if gpu {
            let candidates: &[GUID] = if watermark {
                &[MFVideoFormat_ARGB32, MFVideoFormat_RGB32]
            } else {
                &[MFVideoFormat_NV12, MFVideoFormat_ARGB32, MFVideoFormat_RGB32]
            };
            for sub in candidates {
                for (w, h) in sizes {
                    if let Some((t, on_gpu, gw, gh)) = settle(sub, w, h)? {
                        if on_gpu && gw == w && gh == h {
                            return Ok((t, true));
                        }
                    }
                }
            }
        }
        // Camino por CPU: se acepta el tamaño que conceda el lector, y el resto del pipeline ya
        // trabaja con ese tamaño real.
        for (w, h) in sizes {
            if let Some((t, _, _, _)) = settle(&MFVideoFormat_RGB32, w, h)? {
                return Ok((t, false));
            }
        }
        Err("El decodificador no acepta ningún formato de salida conocido".into())
    }

    // Lee un fotograma de prueba para confirmar que el lector entrega texturas (IMFDXGIBuffer) y
    // rebobina. Barato —un solo frame— y evita quedarse en un NV12 por memoria de sistema, que es
    // justo la combinación que producía la franja verde.
    fn delivers_gpu_textures(reader: &IMFSourceReader, idx: u32) -> std::result::Result<bool, String> {
        let mf = |e: windows::core::Error| format!("{e:?}");
        let mut flags = 0u32;
        let mut sample: Option<IMFSample> = None;
        unsafe { reader.ReadSample(idx, 0, None, Some(&mut flags), None, Some(&mut sample)) }
            .map_err(mf)?;
        let on_gpu = sample
            .and_then(|s| unsafe { s.GetBufferByIndex(0) }.ok())
            .map(|b| b.cast::<IMFDXGIBuffer>().is_ok())
            .unwrap_or(false);
        rewind(reader).map_err(mf)?;
        Ok(on_gpu)
    }

    fn rewind(reader: &IMFSourceReader) -> Result<()> {
        let pos = PROPVARIANT::from(0i64);
        unsafe { reader.SetCurrentPosition(&GUID::zeroed(), &pos) }
    }

    // Escritor de la pista de audio, que se vacía al ritmo del vídeo. El sink MP4 retiene los
    // samples de vídeo mientras el audio no alcanza su marca temporal: escribir primero todo el
    // vídeo y el audio al final hacía crecer sin límite el buffer del muxer. Con texturas de GPU
    // eso además agota el pool del lector y ReadSample se bloquea para siempre. Intercalando, el
    // muxer nunca acumula más que un instante de vídeo.
    struct AudioWriter<'a> {
        stream: u32,
        ranges: Vec<(i64, i64, i64)>,
        seg: usize,
        src: AudioSrc<'a>,
        pending: Option<(IMFSample, i64)>,
        done: bool,
    }

    enum AudioSrc<'a> {
        // Pista del propio fichero: AAC copiado tal cual (gain None) o PCM16 con ganancia.
        Track { reader: &'a IMFSourceReader, idx: u32, gain: Option<f32> },
        // Mezcla sistema+micro ya horneada en memoria, troceada en bloques de 20 ms.
        Mixed { pcm: Vec<i16>, rate: u32, frame: usize, out_t: i64 },
    }

    impl<'a> AudioWriter<'a> {
        fn track(stream: u32, reader: &'a IMFSourceReader, idx: u32, gain: Option<f32>, edit: &ClipEdit) -> Self {
            AudioWriter {
                stream,
                ranges: segment_ranges(edit),
                seg: 0,
                src: AudioSrc::Track { reader, idx, gain },
                pending: None,
                done: false,
            }
        }

        fn mixed(stream: u32, pcm: Vec<i16>, rate: u32, edit: &ClipEdit) -> Self {
            AudioWriter {
                stream,
                ranges: segment_ranges(edit),
                seg: 0,
                src: AudioSrc::Mixed { pcm, rate: rate.max(1), frame: 0, out_t: 0 },
                pending: None,
                done: false,
            }
        }

        // Escribe todo el audio cuya marca de salida no pase de `up_to`. Con i64::MAX, el resto.
        fn advance(
            &mut self,
            sink: &IMFSinkWriter,
            up_to: i64,
            cancel: Option<&std::sync::atomic::AtomicBool>,
        ) -> std::result::Result<(), String> {
            let mf = |e: windows::core::Error| format!("{e:?}");
            while !self.done {
                if cancel.map(|c| c.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false) {
                    return Err(super::CANCELLED.into());
                }
                if self.pending.is_none() {
                    self.pending = self.next_sample()?;
                    if self.pending.is_none() {
                        self.done = true;
                        break;
                    }
                }
                let Some((_, t)) = &self.pending else { break };
                if *t > up_to {
                    break;
                }
                let (sample, _) = self.pending.take().expect("comprobado justo arriba");
                unsafe { sink.WriteSample(self.stream, &sample).map_err(mf)? };
            }
            Ok(())
        }

        fn next_sample(&mut self) -> std::result::Result<Option<(IMFSample, i64)>, String> {
            match &mut self.src {
                AudioSrc::Track { reader, idx, gain } => {
                    next_track_sample(reader, *idx, *gain, &self.ranges, &mut self.seg)
                }
                AudioSrc::Mixed { pcm, rate, frame, out_t } => Ok(next_mixed_sample(
                    pcm, *rate, frame, out_t, &self.ranges, &mut self.seg,
                )),
            }
        }
    }

    // Siguiente sample de la pista del fichero que caiga dentro de un tramo, ya retemporizado.
    fn next_track_sample(
        reader: &IMFSourceReader,
        idx: u32,
        gain: Option<f32>,
        ranges: &[(i64, i64, i64)],
        seg: &mut usize,
    ) -> std::result::Result<Option<(IMFSample, i64)>, String> {
        let mf = |e: windows::core::Error| format!("{e:?}");
        loop {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            unsafe { reader.ReadSample(idx, 0, None, Some(&mut flags), None, Some(&mut sample)) }
                .map_err(mf)?;
            if flags & ENDOFSTREAM != 0 {
                return Ok(None);
            }
            let Some(sample) = sample else { continue };
            let t = unsafe { sample.GetSampleTime().map_err(mf)? };
            while *seg < ranges.len() && t >= ranges[*seg].1 {
                *seg += 1;
            }
            if *seg >= ranges.len() {
                return Ok(None);
            }
            let (start_hns, _, offset) = ranges[*seg];
            if t < start_hns {
                continue;
            }
            let out_t = t - offset;
            unsafe { sample.SetSampleTime(out_t).map_err(mf)? };
            if let Some(g) = gain {
                if (g - 1.0).abs() > 1e-3 {
                    apply_gain_pcm16(&sample, g).map_err(mf)?;
                }
            }
            return Ok(Some((sample, out_t)));
        }
    }

    // Siguiente bloque de la mezcla horneada: solo las porciones conservadas, concatenadas con
    // timestamps secuenciales (el SinkWriter recodifica PCM→AAC). `pcm` es estéreo entrelazado.
    fn next_mixed_sample(
        pcm: &[i16],
        rate: u32,
        frame: &mut usize,
        out_t: &mut i64,
        ranges: &[(i64, i64, i64)],
        seg: &mut usize,
    ) -> Option<(IMFSample, i64)> {
        let frames_total = pcm.len() / 2;
        let block = (rate / 50).max(1) as usize;
        loop {
            if *seg >= ranges.len() {
                return None;
            }
            let (s, e, _) = ranges[*seg];
            let to_frame = |hns: i64| {
                ((hns as f64 / 10_000_000.0 * rate as f64).round().max(0.0) as usize).min(frames_total)
            };
            let (start_f, end_f) = (to_frame(s), to_frame(e));
            if *frame < start_f {
                *frame = start_f;
            }
            if *frame >= end_f {
                *seg += 1;
                *frame = 0;
                continue;
            }
            let n = (end_f - *frame).min(block);
            let bytes = n * 4;
            let sample = unsafe {
                let sample = MFCreateSample().ok()?;
                let buf = MFCreateMemoryBuffer(bytes as u32).ok()?;
                let mut ptr: *mut u8 = std::ptr::null_mut();
                buf.Lock(&mut ptr, None, None).ok()?;
                let dst = std::slice::from_raw_parts_mut(ptr as *mut i16, n * 2);
                dst.copy_from_slice(&pcm[*frame * 2..(*frame + n) * 2]);
                let _ = buf.Unlock();
                buf.SetCurrentLength(bytes as u32).ok()?;
                sample.AddBuffer(&buf).ok()?;
                sample
            };
            let dur = n as i64 * 10_000_000 / rate as i64;
            let time = *out_t;
            unsafe {
                sample.SetSampleTime(time).ok()?;
                sample.SetSampleDuration(dur).ok()?;
            }
            *out_t += dur;
            *frame += n;
            return Some((sample, time));
        }
    }

    // Prepara el escritor de audio del plan elegido. `copy_aac` distingue el camino sin recodificar
    // (la pista se copia tal cual) del que recodifica.
    fn build_audio_writer<'a>(
        sink: &IMFSinkWriter,
        src: &str,
        edit: &ClipEdit,
        plan: &AudioPlan,
        reader: &'a IMFSourceReader,
        remix: Option<&mut Remix>,
        copy_aac: bool,
    ) -> std::result::Result<Option<AudioWriter<'a>>, String> {
        let mf = |e: windows::core::Error| format!("{e:?}");
        match plan {
            AudioPlan::None => Ok(None),
            AudioPlan::Copy(idx) if copy_aac => {
                unsafe { reader.SetStreamSelection(ALL_STREAMS, false).map_err(mf)? };
                unsafe { reader.SetStreamSelection(*idx, true).map_err(mf)? };
                let a_type = unsafe { MFCreateMediaType().map_err(mf)? };
                unsafe {
                    let native = reader.GetNativeMediaType(*idx, 0).map_err(mf)?;
                    native.CopyAllItems(&a_type).map_err(mf)?;
                }
                let stream = unsafe { sink.AddStream(&a_type).map_err(mf)? };
                unsafe { sink.SetInputMediaType(stream, &a_type, None).map_err(mf)? };
                Ok(Some(AudioWriter::track(stream, reader, *idx, None, edit)))
            }
            AudioPlan::Copy(idx) | AudioPlan::Gain(idx, _) => {
                let gain = match plan {
                    AudioPlan::Gain(_, g) => *g,
                    _ => 1.0,
                };
                let (sr, ch) = select_pcm(reader, *idx).map_err(mf)?;
                let stream = add_pcm_audio_stream(sink, sr, ch).map_err(mf)?;
                Ok(Some(AudioWriter::track(stream, reader, *idx, Some(gain), edit)))
            }
            AudioPlan::Remix => {
                let stream = add_remix_audio_stream(sink, planned_remix_rate(src)).map_err(mf)?;
                let (mixed, rate) = remix
                    .ok_or_else(|| "Falta la mezcla de audio".to_string())?
                    .take()?;
                Ok(Some(AudioWriter::mixed(stream, mixed, rate, edit)))
            }
        }
    }

    // Deja el stream de audio seleccionado y en PCM16, y devuelve (rate, canales) reales.
    fn select_pcm(reader: &IMFSourceReader, idx: u32) -> Result<(u32, u32)> {
        unsafe { reader.SetStreamSelection(ALL_STREAMS, false)? };
        unsafe { reader.SetStreamSelection(idx, true)? };
        let native_ch = unsafe {
            reader
                .GetNativeMediaType(idx, 0)
                .ok()
                .and_then(|mt| mt.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).ok())
                .unwrap_or(2)
        };
        let pcm_type = unsafe { MFCreateMediaType()? };
        unsafe {
            pcm_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            pcm_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            pcm_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, native_ch.min(2))?;
            reader.SetCurrentMediaType(idx, None, &pcm_type)?;
        }
        let actual = unsafe { reader.GetCurrentMediaType(idx)? };
        let sr = unsafe { actual.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }.unwrap_or(48000);
        let ch = unsafe { actual.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS) }.unwrap_or(2);
        Ok((sr, ch))
    }

    fn pack2(hi: u32, lo: u32) -> u64 {
        (hi as u64) << 32 | lo as u64
    }

    // Genera la pista de mezcla final aplicando los volúmenes/silencios del editor sobre las
    // pistas de sistema (1) y micro (2). Devuelve PCM16 estéreo entrelazado al rate elegido.
    // El AAC de Media Foundation solo admite 44100/48000 Hz; si el origen es otro, se remuestrea.
    // El AAC de Media Foundation solo admite 44100/48000 Hz: se conserva el del origen si ya es uno
    // de los dos y, si no, se remuestrea a 48000.
    fn remix_rate(sys_rate: u32, mic_rate: u32) -> u32 {
        if sys_rate == 44100 || sys_rate == 48000 {
            sys_rate
        } else if mic_rate == 44100 || mic_rate == 48000 {
            mic_rate
        } else {
            48000
        }
    }

    // Mismo rate que saldrá de la mezcla, leído solo de los tipos nativos (sin decodificar nada).
    // Permite declarar la pista de salida antes de que la mezcla termine, que es lo que deja al
    // audio calcularse en paralelo al vídeo.
    fn planned_remix_rate(src: &str) -> u32 {
        let Ok(reader) = open_reader(src) else { return 48000 };
        let rate = |ordinal: usize| -> u32 {
            audio_stream_at(&reader, ordinal)
                .and_then(|i| unsafe { reader.GetNativeMediaType(i, 0) }.ok())
                .and_then(|mt| unsafe { mt.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND) }.ok())
                .unwrap_or(48000)
        };
        remix_rate(rate(1), rate(0))
    }

    fn build_remixed_pcm(src: &str, edit: &ClipEdit) -> Result<(Vec<i16>, u32)> {
        let (sys_raw, sr, sc) = read_pcm(src, 1)?;
        let (mic_raw, mr, mc) = read_pcm(src, 0)?;

        let out_rate = remix_rate(sr, mr);

        let sys = to_stereo_f32(&sys_raw, sr, sc, out_rate);
        let mic = to_stereo_f32(&mic_raw, mr, mc, out_rate);

        let sys_gain = if edit.mixer.sys_muted { 0.0 } else { edit.mixer.sys_vol.max(0.0) };
        let mic_gain = if edit.mixer.mic_muted { 0.0 } else { edit.mixer.mic_vol.max(0.0) };

        let n = sys.len().max(mic.len());
        let mixed = (0..n)
            .map(|i| {
                let s = sys.get(i).copied().unwrap_or(0.0) * sys_gain;
                let m = mic.get(i).copied().unwrap_or(0.0) * mic_gain;
                soft_clip_sample(s + m)
            })
            .collect();
        Ok((mixed, out_rate))
    }

    // PCM16 entrelazado de `src_ch` canales a estéreo f32 al `out_rate`. Downmix multicanal
    // (frontales íntegros, central/surround a 0.707) y remuestreo lineal cuando los rates
    // difieren. Mismo criterio que el mezclador en vivo (audio.rs), pero offline.
    fn to_stereo_f32(pcm: &[u8], src_rate: u32, src_ch: u16, out_rate: u32) -> Vec<f32> {
        let src_ch = src_ch.max(1) as usize;
        let in_frames = pcm.len() / (src_ch * 2);
        if in_frames == 0 {
            return Vec::new();
        }
        let rd = |frame: usize, ch: usize| -> f32 {
            let idx = (frame * src_ch + ch) * 2;
            i16::from_le_bytes([pcm[idx], pcm[idx + 1]]) as f32
        };
        let lr = |frame: usize| -> (f32, f32) {
            if src_ch == 1 {
                let m = rd(frame, 0);
                (m, m)
            } else {
                let mut l = rd(frame, 0);
                let mut r = rd(frame, 1);
                if src_ch >= 3 {
                    let c = 0.707 * rd(frame, 2);
                    l += c;
                    r += c;
                }
                let mut i = 4;
                while i < src_ch {
                    let s = 0.707 * rd(frame, i);
                    if (i - 4) % 2 == 0 {
                        l += s;
                    } else {
                        r += s;
                    }
                    i += 1;
                }
                (l, r)
            }
        };

        let src_rate_i = src_rate.max(1) as i64;
        let out_rate_i = out_rate.max(1) as i64;
        let same = src_rate_i == out_rate_i;
        let out_frames = if same {
            in_frames
        } else {
            ((in_frames as i64 * out_rate_i) / src_rate_i).max(0) as usize
        };

        let mut out = Vec::with_capacity(out_frames * 2);
        for k in 0..out_frames {
            let (l, r) = if same {
                lr(k.min(in_frames - 1))
            } else {
                let pos = k as f64 * src_rate_i as f64 / out_rate_i as f64;
                let i0 = (pos.floor() as usize).min(in_frames - 1);
                let i1 = (i0 + 1).min(in_frames - 1);
                let frac = (pos - pos.floor()) as f32;
                let (l0, r0) = lr(i0);
                let (l1, r1) = lr(i1);
                (l0 + (l1 - l0) * frac, r0 + (r1 - r0) * frac)
            };
            out.push(l);
            out.push(r);
        }
        out
    }

    // Soft clip: lineal por debajo del umbral, compresión suave por encima. Evita la
    // distorsión áspera del recorte duro al sumar dos fuentes a tope.
    fn soft_clip_sample(x: f32) -> i16 {
        const T: f32 = 0.75;
        let n = x / 32768.0;
        let a = n.abs();
        let y = if a <= T {
            n
        } else {
            n.signum() * (T + (1.0 - T) * (1.0 - (-(a - T) / (1.0 - T)).exp()))
        };
        (y * 32767.0).clamp(-32768.0, 32767.0) as i16
    }

    // Escala en sitio un IMFSample de PCM16 por una ganancia (atenuación del fader de pista única).
    // Solo se invoca cuando la ganancia != 1.0; los samples del SourceReader traen un buffer único.
    fn apply_gain_pcm16(sample: &IMFSample, gain: f32) -> Result<()> {
        let buf = unsafe { sample.ConvertToContiguousBuffer()? };
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut cur = 0u32;
        unsafe { buf.Lock(&mut ptr, None, Some(&mut cur))? };
        let n = cur as usize / 2;
        if n > 0 {
            let s = unsafe { std::slice::from_raw_parts_mut(ptr as *mut i16, n) };
            for x in s.iter_mut() {
                *x = ((*x as f32) * gain).clamp(-32768.0, 32767.0) as i16;
            }
        }
        unsafe { buf.Unlock()? };
        Ok(())
    }

    fn add_remix_audio_stream(sink: &IMFSinkWriter, rate: u32) -> Result<u32> {
        let ch = 2u32;
        let out_type = unsafe { MFCreateMediaType()? };
        unsafe {
            out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            out_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            out_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
            out_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch)?;
            out_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            out_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 128_000 / 8)?;
            out_type.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
        }
        let stream = unsafe { sink.AddStream(&out_type)? };

        let in_type = unsafe { MFCreateMediaType()? };
        unsafe {
            in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            in_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            in_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
            in_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch)?;
            in_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            in_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, ch * 2)?;
            in_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * ch * 2)?;
            sink.SetInputMediaType(stream, &in_type, None)?;
        }
        Ok(stream)
    }

    fn add_pcm_audio_stream(sink: &IMFSinkWriter, rate: u32, ch: u32) -> Result<u32> {
        let out_type = unsafe { MFCreateMediaType()? };
        unsafe {
            out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            out_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            out_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
            out_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch)?;
            out_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            out_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 128_000 / 8)?;
            out_type.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
        }
        let stream = unsafe { sink.AddStream(&out_type)? };
        let in_type = unsafe { MFCreateMediaType()? };
        let block_align = ch * 2;
        unsafe {
            in_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            in_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            in_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
            in_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, ch)?;
            in_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            in_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block_align)?;
            in_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * block_align)?;
            sink.SetInputMediaType(stream, &in_type, None)?;
        }
        Ok(stream)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::editor::{MixerState, Segment};

        fn edit(spans: &[(f64, f64)]) -> ClipEdit {
            ClipEdit {
                segments: spans
                    .iter()
                    .map(|(s, e)| Segment {
                        start_ms: *s,
                        end_ms: *e,
                        pos_ms: None,
                        bound_start_ms: None,
                        bound_end_ms: None,
                        disabled: None,
                        crop_x: None,
                        crop_y: None,
                    })
                    .collect(),
                mixer: MixerState::default(),
                format: Default::default(),
                look: Default::default(),
            }
        }

        // Keyframe cada segundo, como los genera la captura.
        fn probe_of(keyframes: &[f64]) -> ClipProbe {
            ClipProbe {
                frames: Vec::new(),
                keyframes: keyframes.to_vec(),
                in_order: true,
            }
        }

        fn export_fixture(name: &str) -> crate::mp4mux::mf_tests::Read {
            let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
            let dst = std::env::temp_dir().join(format!("fb_export_{}_{name}.mp4", std::process::id()));
            let r = export_clip(
                src.to_string_lossy().into_owned(),
                dst.to_string_lossy().into_owned(),
                edit(&[(0.0, 1500.0)]),
                None,
                None,
                None,
                None,
                |_| {},
            );
            let read = r.map(|_| crate::mp4mux::mf_tests::read(&dst));
            let _ = std::fs::remove_file(&dst);
            read.unwrap_or_else(|e| panic!("{name}: {e}"))
        }

        #[test]
        fn a_webm_with_vp9_and_opus_exports_with_sound() {
            let r = export_fixture("tiny.webm");
            assert!(r.video >= 44, "vídeo {} de 45", r.video);
            assert!(r.audio > 0, "sin audio");
        }

        #[test]
        fn exporting_to_mov_brands_it_as_quicktime() {
            let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.mkv");
            let dst = std::env::temp_dir().join(format!("fb_export_{}_brand.mov", std::process::id()));
            export_clip(
                src.to_string_lossy().into_owned(),
                dst.to_string_lossy().into_owned(),
                edit(&[(0.0, 1500.0)]),
                None,
                None,
                None,
                None,
                |_| {},
            )
            .unwrap();
            let head = std::fs::read(&dst).unwrap()[..12].to_vec();
            let read = crate::mp4mux::mf_tests::read(&dst);
            let _ = std::fs::remove_file(&dst);
            assert_eq!(&head[4..12], b"ftypqt  ");
            assert!(read.video >= 44 && read.audio > 0);
        }

        #[test]
        fn a_cancelled_export_leaves_no_file() {
            let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.webm");
            let dst = std::env::temp_dir().join(format!("fb_export_{}_cancel.mp4", std::process::id()));
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let r = export_clip(
                src.to_string_lossy().into_owned(),
                dst.to_string_lossy().into_owned(),
                edit(&[(0.0, 1500.0)]),
                None,
                None,
                None,
                Some(cancel),
                |_| {},
            );
            let exists = dst.exists();
            let _ = std::fs::remove_file(&dst);
            assert_eq!(r, Err(super::super::CANCELLED.to_string()));
            assert!(!exists);
        }

        #[test]
        fn a_mov_exports_with_sound() {
            let r = export_fixture("tiny.mov");
            assert!(r.video >= 44, "vídeo {} de 45", r.video);
            assert!(r.audio > 0, "sin audio");
        }

        #[test]
        fn an_mkv_with_h264_and_aac_exports_with_sound() {
            let r = export_fixture("tiny.mkv");
            assert!(r.video >= 44, "vídeo {} de 45", r.video);
            assert!(r.audio > 0, "sin audio");
        }

        #[test]
        fn fps_comes_from_the_real_frame_spacing() {
            let mut b_frames: Vec<i64> = [0, 667, 333, 2000, 1333, 1000, 1667].iter().map(|ms| ms * 1000).collect();
            assert_eq!(fps_from_times(&mut b_frames), Some(30));
            let mut ntsc: Vec<i64> = (0..20).map(|i| i * 417_083).collect();
            assert_eq!(fps_from_times(&mut ntsc), Some(24));
            let mut sixty: Vec<i64> = (0..20).map(|i| i * 166_667).collect();
            assert_eq!(fps_from_times(&mut sixty), Some(60));
            assert_eq!(fps_from_times(&mut [0, 333_333]), None);
        }

        #[test]
        fn trimming_only_the_end_can_be_copied() {
            let p = probe_of(&[0.0, 1000.0, 2000.0]);
            assert!(cuts_on_keyframes(&edit(&[(0.0, 1450.0)]), &p, 60));
        }

        #[test]
        fn a_cut_between_keyframes_forces_reencoding() {
            let p = probe_of(&[0.0, 1000.0, 2000.0]);
            assert!(!cuts_on_keyframes(&edit(&[(1300.0, 2000.0)]), &p, 60));
        }

        // Medio fotograma de margen: el editor trabaja en ms con coma flotante.
        #[test]
        fn a_cut_within_half_a_frame_still_counts_as_a_keyframe() {
            let p = probe_of(&[0.0, 1000.0]);
            assert!(cuts_on_keyframes(&edit(&[(1000.2, 2000.0)]), &p, 60));
            assert!(!cuts_on_keyframes(&edit(&[(1012.0, 2000.0)]), &p, 60));
        }

        #[test]
        fn a_clip_without_keyframes_is_never_copied() {
            assert!(!cuts_on_keyframes(&edit(&[(0.0, 1000.0)]), &probe_of(&[]), 60));
        }

        // Los tramos se concatenan sin huecos: el desplazamiento de cada uno acumula lo recortado.
        #[test]
        fn segment_offsets_close_the_gaps() {
            let r = segment_ranges(&edit(&[(0.0, 1000.0), (3000.0, 4000.0)]));
            assert_eq!(r[0], (0, 10_000_000, 0));
            assert_eq!(r[1], (30_000_000, 40_000_000, 20_000_000));
        }

        #[test]
        fn kept_duration_adds_up_the_segments() {
            assert_eq!(kept_hns(&edit(&[(0.0, 1000.0), (3000.0, 4500.0)])), 25_000_000);
        }

        #[test]
        fn remix_rate_keeps_a_valid_source_rate_and_falls_back_to_48k() {
            assert_eq!(remix_rate(44100, 48000), 44100);
            assert_eq!(remix_rate(32000, 48000), 48000);
            assert_eq!(remix_rate(32000, 22050), 48000);
        }

    }
}
