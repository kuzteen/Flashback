use serde::Serialize;

// Estado de la captura para la UI. En la Fase 1 solo sirve para verificar que el
// bucle WGC corre y medir su impacto: cuántos frames llegan y a qué resolución.
#[derive(Serialize, Clone, Default)]
pub struct CaptureStatus {
    pub running: bool,
    pub frames: u64,
    pub width: u32,
    pub height: u32,
    pub seconds: f64,
}

#[derive(Serialize, Clone, Default)]
pub struct MonitorInfo {
    pub id: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    pub thumb: Option<String>,
    // Esquina del monitor en el escritorio virtual. Solo sirve para listarlos en el mismo
    // orden en que están puestos, así que no viaja al frontend.
    #[serde(skip)]
    pub origin: (i32, i32),
}

#[derive(Serialize, Clone, Default)]
pub struct AudioInput {
    pub id: String,
    pub name: String,
}

#[cfg(target_os = "windows")]
pub use win::{
    list_audio_inputs, list_monitors, replay_active, replay_target, save_replay, start,
    start_replay, status, stop, stop_replay,
};

#[cfg(not(target_os = "windows"))]
pub fn list_monitors() -> Vec<MonitorInfo> {
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
pub fn list_audio_inputs() -> Vec<AudioInput> {
    Vec::new()
}

#[cfg(not(target_os = "windows"))]
pub fn start(
    _monitor_id: String,
    _out_dir: String,
    _fps: u32,
    _quality: String,
    _resolution: u32,
    _bitrate: u32,
    _mic: bool,
    _mic_device: String,
    _encoder_pref: String,
) -> Result<bool, String> {
    Err("La captura solo está disponible en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn stop() -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn status() -> CaptureStatus {
    CaptureStatus::default()
}

#[cfg(not(target_os = "windows"))]
pub fn start_replay(
    _monitor_id: String,
    _out_dir: String,
    _seconds: u32,
    _fps: u32,
    _quality: String,
    _resolution: u32,
    _bitrate: u32,
    _mic: bool,
    _mic_device: String,
    _encoder_pref: String,
) -> Result<(), String> {
    Err("El replay solo está disponible en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn stop_replay() {}

#[cfg(not(target_os = "windows"))]
pub fn save_replay(_source: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
pub fn replay_active() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn replay_target() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
mod win;

// Encoders H.264 que no se usan nunca, ni para capturar ni para exportar. "Microsoft AVC DX12
// Encoder HMFT" (la capa D3D12 de Mesa) empieza el vídeo con un intra que no es IDR: ningún
// decodificador lo abre y el clip sale sin imagen sin que nada falle.
pub fn unusable_h264_encoder(name: &str) -> bool {
    name.contains("DX12")
}

// Etiqueta del origen del clip: el juego en modo Aplicación o la pantalla grabada. Mismo texto
// que la etiqueta del selector ("Pantalla N", N de \.\DISPLAYN), sin listar monitores: eso
// fotografía cada pantalla y no pinta nada en pleno guardado.
pub fn source_label(target: Option<&str>) -> String {
    match target {
        Some("window") => crate::detect::current_game().map(|g| g.name).unwrap_or_default(),
        Some(id) => match id.rsplit_once("DISPLAY").and_then(|(_, n)| n.parse::<u32>().ok()) {
            Some(n) => format!("Pantalla {n}"),
            None => "Pantalla".into(),
        },
        None => String::new(),
    }
}

// Graba el origen en cada archivo de la grabación (varias partes si cambió el tamaño a mitad).
pub fn tag_source<'a>(paths: impl IntoIterator<Item = &'a str>, source: &str) {
    if source.is_empty() {
        return;
    }
    for p in paths {
        let _ = crate::library::write_embedded_source(std::path::Path::new(p), source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_are_labelled_by_their_display_number() {
        assert_eq!(source_label(Some(r"\.\DISPLAY2")), "Pantalla 2");
        assert_eq!(source_label(Some("otra-cosa")), "Pantalla");
        assert_eq!(source_label(None), "");
    }

    #[test]
    fn only_the_dx12_fallback_encoder_is_refused() {
        assert!(unusable_h264_encoder("Microsoft AVC DX12 Encoder HMFT"));
        assert!(!unusable_h264_encoder("NVIDIA H.264 Encoder MFT"));
        assert!(!unusable_h264_encoder("Intel Quick Sync Video H.264 Encoder MFT"));
        assert!(!unusable_h264_encoder("AMDh264Encoder"));
        assert!(!unusable_h264_encoder("H264 Encoder MFT"));
    }

    #[test]
    fn every_part_of_a_recording_gets_the_source() {
        let dir = std::env::temp_dir().join(format!("fb_tag_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let parts: Vec<String> = ["a.mp4", "b.mp4"]
            .iter()
            .map(|n| {
                let p = dir.join(n);
                std::fs::write(&p, b"\x00\x00\x00\x08ftyp").unwrap();
                p.to_string_lossy().into_owned()
            })
            .collect();
        tag_source(parts.iter().map(String::as_str), "VALORANT");
        for p in &parts {
            let got = crate::library::read_embedded_source(std::path::Path::new(p));
            assert_eq!(got.as_deref(), Some("VALORANT"), "{p}");
        }
        tag_source(parts.iter().map(String::as_str), "");
        std::fs::remove_dir_all(&dir).ok();
    }
}
