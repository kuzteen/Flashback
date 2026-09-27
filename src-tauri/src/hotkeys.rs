// Atajos globales, atendidos en Rust. Antes pasaban por la interfaz web: el evento iba al
// webview, este pedía el guardado y al volver lanzaba sonido y aviso. Con un juego en primer
// plano Windows le da al webview prioridad de fondo, y en partidas exigentes eso retrasaba el
// clip unos segundos. Además la interfaz se descarga con la ventana cerrada, así que tampoco
// podría atender el de abrir Flashback.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::config::{HotkeyPrefs, ToastTopic};

#[derive(Serialize, Clone)]
struct RecordingChanged {
    recording: bool,
    tapped: bool,
    path: Option<String>,
}

// Mientras se reasigna un atajo en Ajustes se sueltan todos: si no, Windows se traga la
// combinación (RegisterHotKey la intercepta) y nunca llega al campo que la está capturando.
static PAUSED: AtomicBool = AtomicBool::new(false);
static FAILED: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[derive(Clone, Copy)]
enum Action {
    Save,
    Record,
    Open,
}

// Registra los atajos y devuelve los que no se pudieron registrar (combinación ocupada por otra
// app). Cada uno por separado: un conflicto no debe tumbar los demás. Nunca desde el hilo
// principal: el plugin despacha el registro a ese hilo y espera la respuesta.
#[cfg(desktop)]
pub fn apply(app: &AppHandle, keys: &HotkeyPrefs) -> Vec<String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
    let _ = app.global_shortcut().unregister_all();
    if PAUSED.load(Ordering::SeqCst) {
        return Vec::new();
    }
    let mut failed = Vec::new();
    let mut names = Vec::new();
    let es = spanish(app);
    for (accel, action) in [(&keys.save, Action::Save), (&keys.record, Action::Record), (&keys.open, Action::Open)] {
        if accel.is_empty() {
            continue;
        }
        let r = app.global_shortcut().on_shortcut(accel.as_str(), move |app, _, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            // El manejador corre en el hilo principal: abrir la ventana va aquí mismo, el resto a
            // un hilo propio.
            match action {
                Action::Open => crate::show_main(app),
                Action::Save => {
                    let app = app.clone();
                    std::thread::spawn(move || save_clip(&app));
                }
                Action::Record => {
                    let app = app.clone();
                    std::thread::spawn(move || toggle_recording(&app));
                }
            }
        });
        if let Err(e) = r {
            log::warn!("atajos: no se pudo registrar {accel}: {e}");
            failed.push(accel.clone());
            let name = match (action, es) {
                (Action::Save, true) => "guardar clip",
                (Action::Save, false) => "save clip",
                (Action::Record, true) => "grabación",
                (Action::Record, false) => "recording",
                (Action::Open, true) => "abrir Flashback",
                (Action::Open, false) => "open Flashback",
            };
            names.push(format!("{name} ({})", key_labels(accel).join(" + ")));
        }
    }
    if !names.is_empty() {
        let list = names.join(", ");
        let body = if es {
            format!("Atajo en uso por otra app: {list}. Cámbialo en Ajustes.")
        } else {
            format!("Shortcut in use by another app: {list}. Change it in Settings.")
        };
        notify(app, ToastTopic::Problems, "error", body, Vec::new());
    }
    *FAILED.lock().unwrap_or_else(|e| e.into_inner()) = failed.clone();
    failed
}

#[cfg(not(desktop))]
pub fn apply(_app: &AppHandle, _keys: &HotkeyPrefs) -> Vec<String> {
    Vec::new()
}

pub fn failed() -> Vec<String> {
    FAILED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn set_paused(app: &AppHandle, paused: bool) {
    PAUSED.store(paused, Ordering::SeqCst);
    if paused {
        #[cfg(desktop)]
        {
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let _ = app.global_shortcut().unregister_all();
        }
    } else if let Some(p) = crate::session::prefs() {
        apply(app, &p.hotkeys);
    }
}

// Tokens del atajo ("Alt+F8") a las etiquetas de las teclas que pinta el aviso. Misma tabla que
// labelTokens() en la interfaz.
pub fn key_labels(accel: &str) -> Vec<String> {
    accel
        .split('+')
        .map(|t| {
            let fixed = match t {
                "Control" => "CTRL",
                "Alt" => "ALT",
                "Shift" => "SHIFT",
                "Super" => "WIN",
                "Space" => "Espacio",
                "Enter" => "Intro",
                "Backspace" => "⌫",
                "Delete" => "Supr",
                "Insert" => "Ins",
                "Home" => "Inicio",
                "End" => "Fin",
                "PageUp" => "Re Pág",
                "PageDown" => "Av Pág",
                "PrintScreen" => "Impr Pant",
                "ScrollLock" => "Bloq Despl",
                "Pause" => "Pausa",
                "CapsLock" => "Bloq Mayús",
                "NumLock" => "Bloq Num",
                "Up" => "↑",
                "Down" => "↓",
                "Left" => "←",
                "Right" => "→",
                "NumpadAdd" => "Num +",
                "NumpadSubtract" => "Num −",
                "NumpadMultiply" => "Num ×",
                "NumpadDivide" => "Num ÷",
                "NumpadDecimal" => "Num .",
                "NumpadEnter" => "Num Intro",
                "NumpadEqual" => "Num =",
                _ => "",
            };
            if !fixed.is_empty() {
                fixed.to_string()
            } else if let Some(d) = t.strip_prefix("Numpad").filter(|d| d.len() == 1) {
                format!("Num {d}")
            } else {
                t.to_uppercase()
            }
        })
        .collect()
}

fn spanish(app: &AppHandle) -> bool {
    crate::config::get_language(app) == "es"
}

#[cfg(target_os = "windows")]
pub fn notify(app: &AppHandle, topic: ToastTopic, kind: &str, body: String, keys: Vec<String>) {
    use tauri::Manager;
    if !crate::config::get_toast_prefs(app).allows(topic) {
        return;
    }
    if let Some(t) = app.try_state::<crate::toast::Toast>() {
        t.show(crate::toast::ToastData {
            title: "Flashback".into(),
            body,
            keys,
            kind: crate::toast::ToastKind::from_str(kind),
        });
    }
}

#[cfg(not(target_os = "windows"))]
pub fn notify(_app: &AppHandle, _topic: ToastTopic, _kind: &str, _body: String, _keys: Vec<String>) {}

fn toast(app: &AppHandle, topic: ToastTopic, kind: &str, body: String) {
    notify(app, topic, kind, body, Vec::new());
}

fn save_clip(app: &AppHandle) {
    let es = spanish(app);
    if !crate::capture::replay_active() {
        // Sin replay en marcha: o está apagado, o no hay juego ni pantalla que grabar.
        let no_target = crate::session::target(app).is_none();
        let msg = match (no_target, es) {
            (true, true) => "Sin objetivo: selecciona una pantalla o abre un juego para que el replay grabe.",
            (true, false) => "No target: select a screen or open a game for the replay to record.",
            (false, true) => "Activa \"Replay en segundo plano\" en Ajustes para guardar.",
            (false, false) => "Enable \"Background replay\" in Settings to save.",
        };
        toast(app, ToastTopic::Problems, "info", msg.into());
        return;
    }
    let source = crate::capture::source_label(crate::capture::replay_target().as_deref());
    let Some(pending) = crate::capture::begin_save_replay(&source) else {
        let msg = if es {
            "No se pudo guardar el replay (el buffer aún no tiene un keyframe)."
        } else {
            "Could not save the replay (the buffer has no keyframe yet)."
        };
        toast(app, ToastTopic::Problems, "info", msg.into());
        return;
    };
    // El clip ya está fijado en este instante: se avisa ya y se escribe después. Escribir el MP4
    // entero puede llevar segundos con un buffer largo, y el aviso no debe esperar al disco.
    crate::sound::play();
    toast(app, ToastTopic::Saved, "saved", if es { "Clip guardado" } else { "Clip saved" }.into());
    match pending.write() {
        Some(path) => {
            let _ = app.emit("clip-saved", path);
        }
        None => {
            let msg = if es {
                "No se pudo escribir el clip en el disco."
            } else {
                "The clip could not be written to disk."
            };
            toast(app, ToastTopic::Problems, "error", msg.into());
        }
    }
}

fn stop_recording(app: &AppHandle) {
    let es = spanish(app);
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
}

// La grabación colgada del replay usa su encoder: si el replay se re-arma, se cierra antes.
pub fn stop_tapped_recording(app: &AppHandle) {
    let status = crate::capture::status();
    if status.running && status.tapped {
        stop_recording(app);
    }
}

pub fn toggle_recording(app: &AppHandle) {
    let es = spanish(app);
    if crate::capture::status().running {
        stop_recording(app);
        return;
    }

    let prefs = crate::session::prefs().unwrap_or_default();
    let Some(target) = crate::capture::replay_target().or_else(|| crate::session::target(app)) else {
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
    let mic_device = crate::session::mic_device(&prefs);
    let started = crate::capture::start(
        target,
        dir,
        prefs.fps,
        prefs.quality,
        prefs.resolution,
        0,
        prefs.mic,
        mic_device,
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

#[cfg(test)]
mod tests {
    use super::key_labels;

    #[test]
    fn key_labels_match_the_interface() {
        assert_eq!(key_labels("Alt+F8"), ["ALT", "F8"]);
        assert_eq!(key_labels("Control+Numpad3"), ["CTRL", "Num 3"]);
        assert_eq!(key_labels("Shift+NumpadAdd"), ["SHIFT", "Num +"]);
        assert_eq!(key_labels("Super+Space"), ["WIN", "Espacio"]);
        assert_eq!(key_labels("k"), ["K"]);
    }
}
