// Arma y re-arma el replay según los ajustes, el juego en primer plano y la pantalla elegida.
// Antes lo hacía un efecto de la interfaz, y eso obligaba a mantener vivo el webview (unos
// 370 MB) mientras la ventana estaba en la bandeja, que es justo cuando se juega.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, MutexGuard, OnceLock};

use tauri::{AppHandle, Emitter};

use crate::config::{CapturePrefs, ToastTopic};

struct State {
    // None hasta que la interfaz migra sus ajustes: armar antes con los de por defecto pisaría
    // la configuración de quien viene de una versión anterior.
    prefs: Option<CapturePrefs>,
    // Pantalla elegida en la barra; None es el modo Aplicación (la ventana del juego). Dura lo
    // que la sesión, como antes en la interfaz.
    monitor: Option<String>,
    armed: String,
}

static STATE: Mutex<State> = Mutex::new(State { prefs: None, monitor: None, armed: String::new() });
static POKE: OnceLock<Sender<()>> = OnceLock::new();

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn init(app: &AppHandle) {
    let prefs = crate::config::get_capture_prefs(app);
    if let Some(p) = &prefs {
        crate::sound::set_gain(p.sound_gain());
    }
    state().prefs = prefs.clone();
    let (tx, rx) = channel::<()>();
    let _ = POKE.set(tx);
    let handle = app.clone();
    // Un solo hilo re-arma, en orden: arrancar el pipeline tarda y los avisos (cambio de juego,
    // de ajustes) llegan a ráfagas; los que se acumulan mientras tanto cuentan como uno.
    let _ = std::thread::Builder::new().name("flashback-session".into()).spawn(move || {
        if let Some(p) = prefs {
            crate::hotkeys::apply(&handle, &p.hotkeys);
            reconcile(&handle);
        }
        while rx.recv().is_ok() {
            while rx.try_recv().is_ok() {}
            reconcile(&handle);
        }
    });
}

pub fn poke() {
    if let Some(tx) = POKE.get() {
        let _ = tx.send(());
    }
}

pub fn prefs() -> Option<CapturePrefs> {
    state().prefs.clone()
}

// Devuelve los atajos que Windows no dejó registrar.
pub fn set_prefs(app: &AppHandle, prefs: CapturePrefs) -> Result<Vec<String>, String> {
    let prefs = prefs.normalized();
    crate::config::set_capture_prefs(app, &prefs)?;
    let before = state().prefs.replace(prefs.clone());
    crate::sound::set_gain(prefs.sound_gain());
    let failed = if before.as_ref().map(|b| &b.hotkeys) != Some(&prefs.hotkeys) {
        crate::hotkeys::apply(app, &prefs.hotkeys)
    } else {
        crate::hotkeys::failed()
    };
    poke();
    Ok(failed)
}

pub fn monitor() -> Option<String> {
    state().monitor.clone()
}

pub fn set_monitor(monitor: Option<String>) {
    state().monitor = monitor;
    poke();
}

// Lo que se graba: una pantalla elegida, o la ventana del juego detectado si no está desactivado.
pub fn target(app: &AppHandle) -> Option<String> {
    if let Some(m) = monitor() {
        return Some(m);
    }
    let game = crate::detect::current_game()?;
    let disabled = crate::config::get_disabled_games(app).contains(&game.name);
    (!disabled).then(|| "window".to_string())
}

// El micrófono guardado puede haberse desconectado: entonces se usa el primero que haya, como
// hacía la interfaz al listar las entradas.
pub fn mic_device(prefs: &CapturePrefs) -> String {
    let inputs = crate::capture::list_audio_inputs();
    if inputs.iter().any(|d| d.id == prefs.mic_device) {
        prefs.mic_device.clone()
    } else {
        inputs.first().map(|d| d.id.clone()).unwrap_or_default()
    }
}

fn reconcile(app: &AppHandle) {
    let Some(prefs) = prefs() else { return };
    let target = target(app);
    let mic = if prefs.mic { mic_device(&prefs) } else { String::new() };
    let key = match (&target, prefs.replay) {
        (Some(t), true) => {
            // En modo Aplicación el objetivo siempre es "window": el juego va en la clave para que
            // cambiar de juego reconstruya la captura contra la ventana nueva.
            let t = if t == "window" {
                format!("window:{}", crate::detect::current_game().map(|g| g.name).unwrap_or_default())
            } else {
                t.clone()
            };
            format!(
                "{t}|{}|{}|{}|{}|{}|{mic}",
                prefs.seconds, prefs.fps, prefs.quality, prefs.resolution, prefs.mic
            )
        }
        _ => "off".to_string(),
    };
    let was = std::mem::replace(&mut state().armed, key.clone());
    if was == key {
        return;
    }
    crate::hotkeys::stop_tapped_recording(app);
    crate::capture::stop_replay();
    let Some(target) = target.filter(|_| key != "off") else { return };

    let es = crate::config::get_language(app) == "es";
    match start_replay(app, target, &prefs, mic, es) {
        // Solo al armar desde apagado: cambiar calidad o fps reinicia el replay pero no es nuevo.
        Ok(()) if was.is_empty() || was == "off" => ready_toast(app, &prefs, es),
        Ok(()) => {}
        Err(e) => {
            let body = if es { format!("No se pudo iniciar el replay: {e}") } else { format!("Could not start the replay: {e}") };
            crate::hotkeys::notify(app, ToastTopic::Problems, "error", body, Vec::new());
        }
    }
}

fn start_replay(app: &AppHandle, target: String, prefs: &CapturePrefs, mic_device: String, es: bool) -> Result<(), String> {
    let dir = crate::config::clips_dir(app).to_string_lossy().into_owned();
    let encoder = crate::config::get_encoder(app);
    let handle = app.clone();
    let hint = prefs.clone();
    // Al re-apuntar la captura a otra ventana del juego (relevo launcher → juego, cambio a
    // pantalla completa) se vuelve a avisar de que se puede clipear.
    let on_retarget = Box::new(move || {
        let _ = handle.emit("replay-retargeted", ());
        ready_toast(&handle, &hint, crate::config::get_language(&handle) == "es");
    });
    let card_text = if es { "Aquí estaremos cuando vuelvas" } else { "We'll be here when you're back" }.to_string();
    crate::capture::start_replay(
        target,
        dir,
        prefs.seconds,
        prefs.fps,
        prefs.quality.clone(),
        prefs.resolution,
        0,
        prefs.mic,
        mic_device,
        encoder,
        on_retarget,
        card_text,
    )
}

fn ready_toast(app: &AppHandle, prefs: &CapturePrefs, es: bool) {
    let body = if es { "para guardar un clip" } else { "to save a clip" };
    let keys = crate::hotkeys::key_labels(&prefs.hotkeys.save);
    crate::hotkeys::notify(app, ToastTopic::Ready, "ready", body.into(), keys);
}
