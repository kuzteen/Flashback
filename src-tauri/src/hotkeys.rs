// Atajos de guardar clip y grabar, atendidos en Rust. Antes pasaban por la interfaz web: el
// evento iba al webview, este pedía el guardado y al volver lanzaba sonido y aviso. Con un juego
// en primer plano Windows le da al webview prioridad de fondo, y en partidas exigentes eso
// retrasaba el clip unos segundos. Aquí la pulsación guarda, suena y avisa sin tocar el webview;
// la interfaz solo se entera después para refrescar la biblioteca y el estado de grabación.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::config::ToastTopic;

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

pub fn set_record_prefs(prefs: RecordPrefs) {
    *PREFS.lock().unwrap_or_else(|e| e.into_inner()) = Some(prefs);
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
fn toast(app: &AppHandle, topic: ToastTopic, kind: &str, body: String) {
    use tauri::Manager;
    if !crate::config::get_toast_prefs(app).allows(topic) {
        return;
    }
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
fn toast(_app: &AppHandle, _topic: ToastTopic, _kind: &str, _body: String) {}

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
        toast(app, ToastTopic::Problems, "info", msg.into());
        return;
    }
    match crate::capture::save_replay(&crate::capture::source_label(crate::capture::replay_target().as_deref())) {
        Some(path) => {
            crate::sound::play();
            toast(app, ToastTopic::Saved, "saved", if es { "Clip guardado" } else { "Clip saved" }.into());
            let _ = app.emit("clip-saved", path);
        }
        None => {
            let msg = if es {
                "No se pudo guardar el replay (el buffer aún no tiene un keyframe)."
            } else {
                "Could not save the replay (the buffer has no keyframe yet)."
            };
            toast(app, ToastTopic::Problems, "info", msg.into());
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
        match path {
            Some(_) => toast(app, ToastTopic::Saved, "saved", msg.into()),
            None => toast(app, ToastTopic::Recording, "info", msg.into()),
        }
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
        toast(app, ToastTopic::Problems, "info", msg.into());
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
            toast(app, ToastTopic::Recording, "ready", if es { "Grabando" } else { "Recording" }.into());
            let _ = app.emit("recording-changed", RecordingChanged { recording: true, tapped, path: None });
        }
        Err(e) => {
            let msg = if es { format!("No se pudo iniciar la grabación: {e}") } else { format!("Could not start recording: {e}") };
            toast(app, ToastTopic::Problems, "error", msg);
        }
    }
}
