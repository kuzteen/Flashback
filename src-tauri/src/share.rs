use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::editor::ClipEdit;

// Solo puede haber un recodificado de compartir en vuelo: elegir otro tamaño o cerrar el diálogo
// aborta el anterior en vez de dejarlo quemando el encoder para un resultado que ya nadie quiere.
static CURRENT: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

pub fn begin_job() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut cur = CURRENT.lock().unwrap();
    if let Some(prev) = cur.replace(flag.clone()) {
        prev.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    flag
}

pub fn cancel_current() {
    let mut cur = CURRENT.lock().unwrap();
    if let Some(prev) = cur.take() {
        prev.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

// El export siempre escribe una única pista AAC a este bitrate (ver add_remix_audio_stream y
// add_pcm_audio_stream), así que el presupuesto de audio es exacto, no una estimación.
const AUDIO_BPS: u32 = 128_000;

// Suelo de bitrate: por debajo de esto el vídeo no es mirable a ninguna resolución, así que se
// prefiere pasarse del tamaño objetivo antes que entregar un clip inservible.
const MIN_VIDEO_BPS: u32 = 300_000;

const LADDER: [u32; 3] = [1080, 720, 480];

pub struct Target {
    pub bitrate: u32,
    pub max_height: Option<u32>,
}

// Reparto del presupuesto entre vídeo y audio. El 3% se reserva para las cabeceras del contenedor
// y el `moov`, que no entran en el bitrate declarado y aun así ocupan en el archivo final.
pub fn plan(
    target_bytes: u64,
    duration_s: f64,
    src_w: u32,
    src_h: u32,
    src_fps: u32,
    src_bitrate: u32,
) -> Target {
    let secs = duration_s.max(0.1);
    let usable = (target_bytes as f64 * 8.0 * 0.97) - (AUDIO_BPS as f64 * secs);
    // Techo en el bitrate del origen: recodificar por encima de él no recupera calidad que ya se
    // perdió, solo engorda el archivo. Pasa con clips cortos y presets grandes.
    let bitrate = ((usable / secs).max(MIN_VIDEO_BPS as f64) as u32).min(src_bitrate.max(MIN_VIDEO_BPS));

    // Umbral de calidad: por debajo de w*h*fps/60 bits/s la imagen se rompe en bloques. Se prueba la
    // resolución original y, si el presupuesto no la sostiene, se baja por la escalera hasta el
    // primer peldaño que sí aguante; si ninguno lo hace, se queda en el más bajo.
    let mut height = src_h;
    for candidate in std::iter::once(src_h).chain(LADDER.into_iter().filter(|s| *s < src_h)) {
        height = candidate;
        let width = scaled_width(src_w, src_h, candidate);
        let threshold = width as u64 * candidate as u64 * src_fps.max(1) as u64 / 60;
        if (bitrate as u64) >= threshold {
            break;
        }
    }

    Target {
        bitrate,
        max_height: (height < src_h).then_some(height),
    }
}

fn scaled_width(src_w: u32, src_h: u32, height: u32) -> u32 {
    let w = (src_w as u64 * height as u64 / src_h.max(1) as u64) as u32;
    (w + 1) & !1
}

// La edición identidad (un solo tramo que cubre el clip entero, sin silenciar ni bajar faders) no
// cambia nada del archivo, así que recodificarla solo perdería calidad y tiempo.
pub fn is_identity(edit: &ClipEdit, duration_s: f64) -> bool {
    if edit.format != crate::reframe::OutputFormat::Horizontal {
        return false;
    }
    if edit.segments.len() != 1 {
        return false;
    }
    let s = &edit.segments[0];
    if s.disabled.unwrap_or(false) {
        return false;
    }
    let m = &edit.mixer;
    if m.sys_muted || m.mic_muted || (m.sys_vol - 1.0).abs() > 0.001 || (m.mic_vol - 1.0).abs() > 0.001 {
        return false;
    }
    // 50 ms de holgura: la duración del contenedor y la del último frame no cuadran al milisegundo.
    s.start_ms <= 50.0 && s.end_ms >= (duration_s * 1000.0) - 50.0
}

pub fn dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map(|d| d.join("share"))
        .unwrap_or_else(|_| std::env::temp_dir().join("flashback-share"))
}

// El nombre es un hash de todo lo que influye en el resultado, así que arrastrar dos veces el mismo
// clip con el mismo preset reutiliza el archivo en vez de recodificarlo otra vez.
pub fn cache_name(
    src: &Path,
    edit: &ClipEdit,
    target_bytes: Option<u64>,
    watermark: Option<&str>,
) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.to_string_lossy().hash(&mut h);
    src.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .hash(&mut h);
    serde_json::to_string(edit).unwrap_or_default().hash(&mut h);
    target_bytes.hash(&mut h);
    watermark.hash(&mut h);
    format!("{:016x}.mp4", h.finish())
}

// Los temporales solo tienen que sobrevivir a la sesión en la que se comparte. Purgarlos al arrancar
// evita que la carpeta crezca sin control sin tener que llevar contabilidad de cuáles siguen en uso.
pub fn cleanup(dir: &Path) {
    const MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|t| t.elapsed().map(|e| e > MAX_AGE).unwrap_or(false))
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(start: f64, end: f64) -> ClipEdit {
        ClipEdit {
            segments: vec![crate::editor::Segment {
                start_ms: start,
                end_ms: end,
                pos_ms: None,
                bound_start_ms: None,
                bound_end_ms: None,
                disabled: None,
                crop_x: None,
            }],
            mixer: Default::default(),
            format: Default::default(),
        }
    }

    const SRC_BPS: u32 = 40_000_000;

    #[test]
    fn ten_mb_of_a_minute_fits_the_budget() {
        let t = plan(10 * 1024 * 1024, 60.0, 1920, 1080, 60, SRC_BPS);
        let total_bits = (t.bitrate + AUDIO_BPS) as u64 * 60;
        assert!(total_bits / 8 <= 10 * 1024 * 1024);
    }

    #[test]
    fn tight_budget_drops_resolution() {
        // 10 MB para un minuto son ~1,2 Mbps: no sostienen 1080p60, pero sí 720p60.
        let t = plan(10 * 1024 * 1024, 60.0, 1920, 1080, 60, SRC_BPS);
        assert_eq!(t.max_height, Some(720));
    }

    #[test]
    fn hopeless_budget_lands_on_the_bottom_step() {
        let t = plan(10 * 1024 * 1024, 900.0, 1920, 1080, 60, SRC_BPS);
        assert_eq!(t.max_height, Some(480));
    }

    #[test]
    fn roomy_budget_keeps_resolution() {
        let t = plan(100 * 1024 * 1024, 20.0, 1920, 1080, 60, SRC_BPS);
        assert_eq!(t.max_height, None);
    }

    #[test]
    fn never_upscales() {
        let t = plan(10 * 1024 * 1024, 60.0, 854, 480, 30, SRC_BPS);
        assert_eq!(t.max_height, None);
    }

    #[test]
    fn bitrate_never_drops_below_the_floor() {
        let t = plan(1024 * 1024, 600.0, 1920, 1080, 60, SRC_BPS);
        assert_eq!(t.bitrate, MIN_VIDEO_BPS);
    }

    // Clip corto con preset grande: el presupuesto daría de sobra, pero subir del bitrate de origen
    // solo engordaría el archivo sin recuperar calidad.
    #[test]
    fn bitrate_never_exceeds_the_source() {
        let t = plan(100 * 1024 * 1024, 5.0, 1920, 1080, 60, 8_000_000);
        assert_eq!(t.bitrate, 8_000_000);
    }

    #[test]
    fn source_cap_still_respects_the_floor() {
        let t = plan(100 * 1024 * 1024, 5.0, 1920, 1080, 60, 1_000);
        assert_eq!(t.bitrate, MIN_VIDEO_BPS);
    }

    #[test]
    fn full_span_is_identity() {
        assert!(is_identity(&edit(0.0, 60_000.0), 60.0));
    }

    #[test]
    fn trimmed_span_is_not_identity() {
        assert!(!is_identity(&edit(2_000.0, 60_000.0), 60.0));
        assert!(!is_identity(&edit(0.0, 45_000.0), 60.0));
    }

    #[test]
    fn muted_track_is_not_identity() {
        let mut e = edit(0.0, 60_000.0);
        e.mixer.mic_muted = true;
        assert!(!is_identity(&e, 60.0));
    }
}
