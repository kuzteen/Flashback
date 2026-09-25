// Sonido de "clip guardado": el de Flashback o uno elegido por el usuario. Lo reproduce Windows
// desde memoria con el volumen ya aplicado a las muestras (PlaySound no tiene control de volumen).
// El archivo del usuario se decodifica una sola vez al elegirlo y queda como borrador mientras se
// escoge el tramo de 2 s; el atajo solo reproduce un búfer ya preparado.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub const MAX_SECS: f64 = 2.0;
// Por debajo no se oye un aviso, solo un clic.
const MIN_SECS: f64 = 0.2;
// Tope de lo que se decodifica para elegir el tramo: una canción entera no hace falta y ocuparía
// cientos de MB en memoria.
const MAX_SOURCE_SECS: f64 = 300.0;
const PEAKS: usize = 480;
const FADE_IN_SECS: f64 = 0.005;
const FADE_OUT_SECS: f64 = 0.015;

#[cfg(target_os = "windows")]
static DEFAULT_WAV: &[u8] = include_bytes!("../../static/sounds/replay-saved.wav");

struct State {
    gain: f32,
    custom: Option<Arc<Vec<u8>>>,
    // PlaySound lee estos búferes mientras suenan, así que solo se sueltan después de pararlo.
    prepared: Option<(u32, bool, Arc<Vec<u8>>)>,
    preview: Option<Arc<Vec<u8>>>,
}

static STATE: Mutex<State> = Mutex::new(State { gain: 0.55, custom: None, prepared: None, preview: None });

struct Draft {
    name: String,
    pcm: Vec<u8>,
    sr: u32,
    ch: u16,
}

static DRAFT: Mutex<Option<Draft>> = Mutex::new(None);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftInfo {
    pub name: String,
    pub duration_ms: u64,
    pub peaks: Vec<f32>,
    pub truncated: bool,
}

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn custom_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("sounds").join("save-sound.wav"))
}

pub fn init(app: &tauri::AppHandle) {
    if crate::config::get_save_sound_name(app).is_none() {
        return;
    }
    if let Some(bytes) = custom_path(app).and_then(|p| std::fs::read(p).ok()) {
        state().custom = Some(Arc::new(bytes));
    }
}

pub fn set_gain(gain: f32) {
    state().gain = gain.clamp(0.0, 1.0);
}

pub fn play() {
    let mut s = state();
    if s.gain <= 0.0 {
        return;
    }
    let key = (s.gain * 1000.0).round() as u32;
    let custom = s.custom.is_some();
    let ready = matches!(&s.prepared, Some((k, c, _)) if *k == key && *c == custom);
    if !ready {
        stop_playback();
        let scaled = match &s.custom {
            Some(c) => scaled_wav(c, s.gain),
            None => scaled_wav(default_wav(), s.gain),
        };
        s.prepared = Some((key, custom, Arc::new(scaled)));
    }
    if let Some((_, _, buf)) = &s.prepared {
        start_playback(buf);
    }
}

pub fn open_draft(src: &str) -> Result<DraftInfo, String> {
    let (pcm, sr, ch) = decode_head(src, MAX_SOURCE_SECS)?;
    let frame = ch.max(1) as usize * 2;
    let frames = pcm.len() / frame;
    if frames == 0 || sr == 0 {
        return Err("El archivo no tiene audio".into());
    }
    let name = std::path::Path::new(src)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| src.to_string());
    let info = DraftInfo {
        name: name.clone(),
        duration_ms: frames as u64 * 1000 / sr as u64,
        peaks: peaks(&pcm, ch, PEAKS),
        truncated: frames as f64 >= MAX_SOURCE_SECS * sr as f64 - 1.0,
    };
    *DRAFT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Draft { name, pcm, sr, ch });
    Ok(info)
}

pub fn preview_draft(start_ms: u64, len_ms: u64) {
    let wav = {
        let d = DRAFT.lock().unwrap_or_else(|e| e.into_inner());
        let Some(d) = d.as_ref() else { return };
        wav_bytes(&cut(d, start_ms, len_ms), d.sr, d.ch)
    };
    let mut s = state();
    // Con el sonido apagado se escucha al volumen medio: el usuario está eligiendo, no avisando.
    let gain = if s.gain > 0.0 { s.gain } else { 0.55 };
    stop_playback();
    let buf = Arc::new(scaled_wav(&wav, gain));
    start_playback(&buf);
    s.preview = Some(buf);
}

pub fn accept_draft(app: &tauri::AppHandle, start_ms: u64, len_ms: u64) -> Result<String, String> {
    let draft = DRAFT.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or("No hay sonido elegido")?;
    let bytes = wav_bytes(&cut(&draft, start_ms, len_ms), draft.sr, draft.ch);
    let dst = custom_path(app).ok_or("No hay carpeta de datos")?;
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&dst, &bytes).map_err(|e| e.to_string())?;
    crate::config::set_save_sound_name(app, Some(&draft.name))?;
    let mut s = state();
    stop_playback();
    s.preview = None;
    s.prepared = None;
    s.custom = Some(Arc::new(bytes));
    Ok(draft.name)
}

pub fn discard_draft() {
    DRAFT.lock().unwrap_or_else(|e| e.into_inner()).take();
    let mut s = state();
    stop_playback();
    s.preview = None;
}

// Tramo de `len_ms` (entre MIN_SECS y MAX_SECS) que empieza en `start_ms`, desplazado hacia atrás
// si se sale del final.
fn cut(d: &Draft, start_ms: u64, len_ms: u64) -> Vec<u8> {
    let frame = d.ch.max(1) as usize * 2;
    let frames = d.pcm.len() / frame;
    let secs = (len_ms as f64 / 1000.0).clamp(MIN_SECS, MAX_SECS);
    let len = ((secs * d.sr as f64) as usize).clamp(1, frames);
    let start = ((start_ms as f64 / 1000.0 * d.sr as f64) as usize).min(frames - len);
    let mut pcm = d.pcm[start * frame..(start + len) * frame].to_vec();
    fade_in(&mut pcm, d.sr, d.ch, FADE_IN_SECS);
    fade_out(&mut pcm, d.sr, d.ch, FADE_OUT_SECS);
    pcm
}

fn peaks(pcm: &[u8], ch: u16, buckets: usize) -> Vec<f32> {
    let frame = ch.max(1) as usize * 2;
    let frames = pcm.len() / frame;
    let n = buckets.min(frames).max(1);
    (0..n)
        .map(|b| {
            let from = b * frames / n;
            let to = ((b + 1) * frames / n).max(from + 1);
            pcm[from * frame..to * frame]
                .chunks_exact(2)
                .map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs())
                .max()
                .unwrap_or(0) as f32
                / 32768.0
        })
        .collect()
}

fn wav_bytes(pcm: &[u8], sr: u32, ch: u16) -> Vec<u8> {
    let block = ch * 2;
    let mut v = Vec::with_capacity(44 + pcm.len());
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&ch.to_le_bytes());
    v.extend_from_slice(&sr.to_le_bytes());
    v.extend_from_slice(&(sr * block as u32).to_le_bytes());
    v.extend_from_slice(&block.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    v.extend_from_slice(pcm);
    v
}

pub fn clear_custom(app: &tauri::AppHandle) -> Result<(), String> {
    {
        let mut s = state();
        stop_playback();
        s.prepared = None;
        s.preview = None;
        s.custom = None;
    }
    if let Some(p) = custom_path(app) {
        let _ = std::fs::remove_file(p);
    }
    crate::config::set_save_sound_name(app, None)
}

#[cfg(target_os = "windows")]
fn default_wav() -> &'static [u8] {
    DEFAULT_WAV
}

#[cfg(not(target_os = "windows"))]
fn default_wav() -> &'static [u8] {
    &[]
}

#[cfg(target_os = "windows")]
fn start_playback(buf: &[u8]) {
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    unsafe {
        let _ = PlaySoundW(PCWSTR(buf.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(target_os = "windows")]
fn stop_playback() {
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_FLAGS};
    unsafe {
        let _ = PlaySoundW(PCWSTR::null(), None, SND_FLAGS(0));
    }
}

#[cfg(target_os = "windows")]
fn decode_head(src: &str, secs: f64) -> Result<(Vec<u8>, u32, u16), String> {
    crate::editor::decode_audio_head(src.to_string(), secs)
        .map_err(|_| "No se pudo leer el audio de ese archivo".to_string())
}

#[cfg(not(target_os = "windows"))]
fn start_playback(_buf: &[u8]) {}

#[cfg(not(target_os = "windows"))]
fn stop_playback() {}

#[cfg(not(target_os = "windows"))]
fn decode_head(_src: &str, _secs: f64) -> Result<(Vec<u8>, u32, u16), String> {
    Err("Solo disponible en Windows".into())
}

// Un corte cae casi siempre en mitad de la onda y sonaría como un chasquido.
fn fade_in(pcm: &mut [u8], sr: u32, ch: u16, secs: f64) {
    let frame = ch.max(1) as usize * 2;
    let frames = pcm.len() / frame;
    let n = ((secs * sr as f64) as usize).min(frames);
    for i in 0..n {
        let gain = i as f32 / n as f32;
        for s in pcm[i * frame..(i + 1) * frame].chunks_exact_mut(2) {
            let v = i16::from_le_bytes([s[0], s[1]]) as f32 * gain;
            s.copy_from_slice(&(v.round() as i16).to_le_bytes());
        }
    }
}

fn fade_out(pcm: &mut [u8], sr: u32, ch: u16, secs: f64) {
    let frame = ch.max(1) as usize * 2;
    let frames = pcm.len() / frame;
    let n = ((secs * sr as f64) as usize).min(frames);
    for i in 0..n {
        let gain = (n - i - 1) as f32 / n as f32;
        let at = (frames - n + i) * frame;
        for s in pcm[at..at + frame].chunks_exact_mut(2) {
            let v = i16::from_le_bytes([s[0], s[1]]) as f32 * gain;
            s.copy_from_slice(&(v.round() as i16).to_le_bytes());
        }
    }
}

// Copia del WAV (PCM de 16 bits) con las muestras del bloque `data` multiplicadas por `gain`.
fn scaled_wav(wav: &[u8], gain: f32) -> Vec<u8> {
    let mut out = wav.to_vec();
    let mut i = 12;
    while i + 8 <= out.len() {
        let size = u32::from_le_bytes([out[i + 4], out[i + 5], out[i + 6], out[i + 7]]) as usize;
        let body = i + 8;
        if &out[i..i + 4] == b"data" {
            let end = (body + size).min(out.len());
            for s in out[body..end].chunks_exact_mut(2) {
                let v = i16::from_le_bytes([s[0], s[1]]) as f32 * gain;
                s.copy_from_slice(&(v.round().clamp(-32768.0, 32767.0) as i16).to_le_bytes());
            }
            break;
        }
        i = body + size + (size & 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(samples: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut v = b"RIFF\0\0\0\0WAVE".to_vec();
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&[1, 0, 1, 0, 0x80, 0xBB, 0, 0, 0, 0x77, 1, 0, 2, 0, 16, 0]);
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(&data);
        v
    }

    fn samples(pcm: &[u8]) -> Vec<i16> {
        pcm.chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
    }

    #[test]
    fn the_volume_scales_only_the_samples() {
        let src = wav(&[1000, -1000, 32767]);
        let out = scaled_wav(&src, 0.5);
        assert_eq!(out.len(), src.len());
        assert_eq!(&out[..out.len() - 6], &src[..src.len() - 6]);
        assert_eq!(samples(&out[out.len() - 6..]), vec![500, -500, 16384]);
    }

    #[test]
    fn fade_out_ramps_the_tail_to_silence_on_every_channel() {
        let mut pcm: Vec<u8> = [1000i16; 12].iter().flat_map(|s| s.to_le_bytes()).collect();
        fade_out(&mut pcm, 4, 2, 1.0);
        assert_eq!(samples(&pcm), vec![1000, 1000, 1000, 1000, 750, 750, 500, 500, 250, 250, 0, 0]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn a_long_file_is_decoded_only_up_to_the_limit() {
        let dir = std::env::temp_dir().join(format!("flashback-sound-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("long.wav");
        let pcm: Vec<u8> = (0..48_000 * 3 * 2).flat_map(|i| ((i % 200) as i16 * 100).to_le_bytes()).collect();
        std::fs::write(&src, wav_bytes(&pcm, 48_000, 2)).unwrap();
        let (head, sr, ch) = decode_head(&src.to_string_lossy(), MAX_SECS).unwrap();
        assert_eq!((sr, ch), (48_000, 2));
        assert_eq!(head.len(), (MAX_SECS * 48_000.0) as usize * 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn draft(frames: usize) -> Draft {
        let pcm = (0..frames as i16).flat_map(|i| [i.to_le_bytes(), i.to_le_bytes()].concat()).collect();
        Draft { name: "x".into(), pcm, sr: 1000, ch: 2 }
    }

    #[test]
    fn the_cut_is_two_seconds_and_slides_back_from_the_end() {
        let d = draft(5000);
        let at = |pcm: &[u8], f: usize| i16::from_le_bytes([pcm[f * 4], pcm[f * 4 + 1]]);
        let c = cut(&d, 1000, 2000);
        assert_eq!(c.len(), 2000 * 4);
        assert_eq!(at(&c, 100), 1100);
        let c = cut(&d, 4500, 2000);
        assert_eq!(c.len(), 2000 * 4);
        assert_eq!(at(&c, 100), 3100);
    }

    #[test]
    fn a_sound_shorter_than_the_limit_is_kept_whole() {
        assert_eq!(cut(&draft(700), 300, 2000).len(), 700 * 4);
    }

    #[test]
    fn the_length_is_the_chosen_one_within_the_limits() {
        let d = draft(5000);
        assert_eq!(cut(&d, 0, 1200).len(), 1200 * 4);
        assert_eq!(cut(&d, 0, 5000).len(), 2000 * 4);
        assert_eq!(cut(&d, 0, 10).len(), 200 * 4);
        let c = cut(&d, 4500, 1200);
        assert_eq!(i16::from_le_bytes([c[400], c[401]]), 3900);
    }

    #[test]
    fn peaks_are_normalized_and_capped_to_the_bucket_count() {
        let p = peaks(&draft(5000).pcm, 2, 10);
        assert_eq!(p.len(), 10);
        assert!(p.windows(2).all(|w| w[0] <= w[1]));
        assert!((p[9] - 4999.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn fade_out_on_a_sound_shorter_than_the_fade_does_not_panic() {
        let mut pcm: Vec<u8> = [1000i16; 2].iter().flat_map(|s| s.to_le_bytes()).collect();
        fade_out(&mut pcm, 48_000, 2, FADE_OUT_SECS);
        assert_eq!(samples(&pcm), vec![0, 0]);
    }
}
