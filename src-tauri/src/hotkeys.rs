// Atajos de guardar clip y grabar, atendidos en Rust. Antes pasaban por la interfaz web: el
// evento iba al webview, este pedía el guardado y al volver lanzaba sonido y aviso. Con un juego
// en primer plano Windows le da al webview prioridad de fondo, y en partidas exigentes eso
// retrasaba el clip unos segundos. Aquí la pulsación guarda, suena y avisa sin tocar el webview;
// la interfaz solo se entera después para refrescar la biblioteca y el estado de grabación.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

// Ajustes de la grabación manual, que la interfaz mantiene al día. Con el replay activo la
// grabación se engancha a él y solo hace falta su objetivo; sin replay se usan todos.
#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecordPrefs {
    pub target: Option<String>,
    pub fps: u32,
    pub quality: String,
    pub resolution: u32,
    pub mic: bool,
    pub mic_device: String,
}

#[derive(Serialize, Clone)]
struct RecordingChanged {
    recording: bool,
    tapped: bool,
    path: Option<String>,
}

static PREFS: Mutex<Option<RecordPrefs>> = Mutex::new(None);
static SOUND_GAIN: Mutex<f32> = Mutex::new(0.55);

pub fn set_record_prefs(prefs: RecordPrefs) {
    *PREFS.lock().unwrap_or_else(|e| e.into_inner()) = Some(prefs);
}

pub fn set_sound_gain(gain: f32) {
    *SOUND_GAIN.lock().unwrap_or_else(|e| e.into_inner()) = gain.clamp(0.0, 1.0);
}

#[derive(Clone, Copy)]
enum Action {
    Save,
    Record,
}

// Registra los dos atajos y devuelve los que no se pudieron registrar (combinación ocupada por
// otra app). Cada uno por separado: un conflicto no debe tumbar el otro.
#[cfg(desktop)]
pub fn register(app: &AppHandle, save: &str, record: &str) -> Vec<String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let mut failed = Vec::new();
    for (accel, action) in [(save, Action::Save), (record, Action::Record)] {
        if accel.is_empty() {
            continue;
        }
        // El manejador corre en el hilo principal de la app: el trabajo va a un hilo propio.
        let r = app.global_shortcut().on_shortcut(accel, move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                let app = app.clone();
                std::thread::spawn(move || match action {
                    Action::Save => save_clip(&app),
                    Action::Record => toggle_recording(&app),
                });
            }
        });
        if r.is_err() {
            failed.push(accel.to_string());
        }
    }
    failed
}

fn spanish(app: &AppHandle) -> bool {
    crate::config::get_language(app) == "es"
}

#[cfg(target_os = "windows")]
fn toast(app: &AppHandle, kind: &str, body: String) {
    use tauri::Manager;
    if let Some(t) = app.try_state::<crate::toast::Toast>() {
        t.show(crate::toast::ToastData {
            title: "Flashback".into(),
            body,
            keys: Vec::new(),
            kind: crate::toast::ToastKind::from_str(kind),
        });
    }
}

#[cfg(not(target_os = "windows"))]
fn toast(_app: &AppHandle, _kind: &str, _body: String) {}

fn save_clip(app: &AppHandle) {
    let es = spanish(app);
    if !crate::capture::replay_active() {
        // Sin replay en marcha: o está apagado, o no hay juego ni pantalla que grabar.
        let no_target = PREFS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|p| p.target.is_none());
        let msg = match (no_target, es) {
            (true, true) => "Sin objetivo: selecciona una pantalla o abre un juego para que el replay grabe.",
            (true, false) => "No target: select a screen or open a game for the replay to record.",
            (false, true) => "Activa \"Replay en segundo plano\" en Ajustes para guardar.",
            (false, false) => "Enable \"Background replay\" in Settings to save.",
        };
        toast(app, "info", msg.into());
        return;
    }
    match crate::capture::save_replay(&crate::capture::source_label(crate::capture::replay_target().as_deref())) {
        Some(path) => {
            play_saved_sound(*SOUND_GAIN.lock().unwrap_or_else(|e| e.into_inner()));
            toast(app, "saved", if es { "Clip guardado" } else { "Clip saved" }.into());
            let _ = app.emit("clip-saved", path);
        }
        None => {
            let msg = if es {
                "No se pudo guardar el replay (el buffer aún no tiene un keyframe)."
            } else {
                "Could not save the replay (the buffer has no keyframe yet)."
            };
            toast(app, "info", msg.into());
        }
    }
}

fn toggle_recording(app: &AppHandle) {
    let es = spanish(app);
    if crate::capture::status().running {
        let path = crate::capture::stop();
        let msg = match (&path, es) {
            (Some(_), true) => "Clip guardado",
            (Some(_), false) => "Clip saved",
            (None, true) => "Grabación detenida",
            (None, false) => "Recording stopped",
        };
        toast(app, if path.is_some() { "saved" } else { "info" }, msg.into());
        let _ = app.emit("recording-changed", RecordingChanged { recording: false, tapped: false, path });
        return;
    }

    let prefs = PREFS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default();
    let Some(target) = crate::capture::replay_target().or(prefs.target.clone()) else {
        let msg = if es {
            "Selecciona una pantalla para grabar, o abre un juego para el modo Aplicación."
        } else {
            "Select a screen to record, or open a game for Application mode."
        };
        toast(app, "info", msg.into());
        return;
    };
    let dir = crate::config::clips_dir(app).to_string_lossy().into_owned();
    let encoder = crate::config::get_encoder(app);
    let started = crate::capture::start(
        target,
        dir,
        prefs.fps,
        prefs.quality,
        prefs.resolution,
        0,
        prefs.mic,
        prefs.mic_device,
        encoder,
    );
    match started {
        Ok(tapped) => {
            toast(app, "ready", if es { "Grabando" } else { "Recording" }.into());
            let _ = app.emit("recording-changed", RecordingChanged { recording: true, tapped, path: None });
        }
        Err(e) => {
            let msg = if es { format!("No se pudo iniciar la grabación: {e}") } else { format!("Could not start recording: {e}") };
            toast(app, "error", msg);
        }
    }
}

// Sonido de "clip guardado" reproducido por Windows desde memoria, con el volumen ya aplicado a
// las muestras (PlaySound no tiene control de volumen). Cada nivel se prepara una vez y se
// conserva: la reproducción es asíncrona y lee el búfer mientras suena.
#[cfg(target_os = "windows")]
fn play_saved_sound(gain: f32) {
    use std::collections::HashMap;
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

    static WAV: &[u8] = include_bytes!("../../static/sounds/replay-saved.wav");
    static SCALED: Mutex<Option<HashMap<u32, &'static [u8]>>> = Mutex::new(None);

    let key = (gain * 1000.0).round() as u32;
    let buf = {
        let mut cache = SCALED.lock().unwrap_or_else(|e| e.into_inner());
        let map = cache.get_or_insert_with(HashMap::new);
        *map.entry(key).or_insert_with(|| Box::leak(scaled_wav(WAV, gain).into_boxed_slice()))
    };
    unsafe {
        let _ = PlaySoundW(PCWSTR(buf.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(not(target_os = "windows"))]
fn play_saved_sound(_gain: f32) {}

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
    use super::scaled_wav;

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

    #[test]
    fn the_volume_scales_only_the_samples() {
        let src = wav(&[1000, -1000, 32767]);
        let out = scaled_wav(&src, 0.5);
        assert_eq!(out.len(), src.len());
        assert_eq!(&out[..out.len() - 6], &src[..src.len() - 6]);
        let samples: Vec<i16> = out[out.len() - 6..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(samples, vec![500, -500, 16384]);
    }
}
