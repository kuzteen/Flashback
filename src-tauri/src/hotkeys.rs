// Atajos globales, atendidos en Rust. Antes pasaban por la interfaz web: el evento iba al
// webview, este pedía el guardado y al volver lanzaba sonido y aviso. Con un juego en primer
// plano Windows le da al webview prioridad de fondo, y en partidas exigentes eso retrasaba el
// clip unos segundos. Además la interfaz se descarga con la ventana cerrada, así que tampoco
// podría atender el de abrir Flashback.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
static FAILED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

#[derive(Clone, Copy)]
enum Action {
    Save,
    Record,
    Open,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Save => "guardar",
            Action::Record => "grabar",
            Action::Open => "abrir",
        }
    }
}

// Algunos juegos (LoL con Vanguard, o cualquiera que corra elevado) impiden que RegisterHotKey
// entregue la tecla a otras apps. El sondeo del estado del teclado la ve igualmente, así que
// convive con el registro y cada pulsación se atiende por el camino que llegue primero.
const DEDUP_MS: u64 = 300;
static LAST_FIRE: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];

fn now_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

fn trigger(app: &AppHandle, action: Action, via: &str) {
    let now = now_ms();
    let fresh = LAST_FIRE[action as usize]
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |prev| {
            (prev == 0 || now.saturating_sub(prev) >= DEDUP_MS).then_some(now)
        })
        .is_ok();
    if !fresh {
        log::info!("atajos: {} también por {via}", action.label());
        return;
    }
    log::info!("atajos: {} por {via}", action.label());
    match action {
        Action::Open => {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || crate::show_main(&handle));
        }
        Action::Save => {
            let app = app.clone();
            std::thread::spawn(move || save_clip(&app));
        }
        Action::Record => {
            let app = app.clone();
            std::thread::spawn(move || toggle_recording(&app));
        }
    }
}

#[derive(Clone, Copy)]
struct Combo {
    action: Action,
    mods: u8,
    vk: u16,
}

const MOD_CTRL: u8 = 1;
const MOD_ALT: u8 = 2;
const MOD_SHIFT: u8 = 4;
const MOD_WIN: u8 = 8;

static COMBOS: Mutex<Vec<Combo>> = Mutex::new(Vec::new());

fn token_vk(t: &str) -> Option<u16> {
    let fixed = match t {
        "Space" => 0x20,
        "Enter" => 0x0D,
        "Tab" => 0x09,
        "Backspace" => 0x08,
        "Delete" => 0x2E,
        "Insert" => 0x2D,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "PrintScreen" => 0x2C,
        "ScrollLock" => 0x91,
        "Pause" => 0x13,
        "CapsLock" => 0x14,
        "NumLock" => 0x90,
        "Up" => 0x26,
        "Down" => 0x28,
        "Left" => 0x25,
        "Right" => 0x27,
        "NumpadAdd" => 0x6B,
        "NumpadSubtract" => 0x6D,
        "NumpadMultiply" => 0x6A,
        "NumpadDivide" => 0x6F,
        "NumpadDecimal" => 0x6E,
        "NumpadEnter" => 0x0D,
        "`" => 0xC0,
        "-" => 0xBD,
        "=" => 0xBB,
        "[" => 0xDB,
        "]" => 0xDD,
        "\\" => 0xDC,
        ";" => 0xBA,
        "'" => 0xDE,
        "," => 0xBC,
        "." => 0xBE,
        "/" => 0xBF,
        _ => 0,
    };
    if fixed != 0 {
        return Some(fixed);
    }
    if let Some(d) = t.strip_prefix("Numpad").filter(|d| d.len() == 1) {
        return d.parse::<u16>().ok().map(|n| 0x60 + n);
    }
    if let Some(n) = t.strip_prefix('F').and_then(|n| n.parse::<u16>().ok()).filter(|n| (1..=24).contains(n)) {
        return Some(0x70 + n - 1);
    }
    let mut chars = t.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase() as u16),
        _ => None,
    }
}

fn parse_combo(action: Action, accel: &str) -> Option<Combo> {
    let mut mods = 0;
    let mut vk = None;
    for t in accel.split('+') {
        match t {
            "Control" | "Ctrl" => mods |= MOD_CTRL,
            "Alt" => mods |= MOD_ALT,
            "Shift" => mods |= MOD_SHIFT,
            "Super" => mods |= MOD_WIN,
            _ => vk = Some(token_vk(t)?),
        }
    }
    vk.map(|vk| Combo { action, mods, vk })
}

#[cfg(target_os = "windows")]
fn key_down(vk: u16) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    unsafe { (GetAsyncKeyState(vk as i32) as u16) & 0x8000 != 0 }
}

#[cfg(target_os = "windows")]
fn mods_down() -> u8 {
    let mut m = 0;
    if key_down(0x11) {
        m |= MOD_CTRL;
    }
    if key_down(0x12) {
        m |= MOD_ALT;
    }
    if key_down(0x10) {
        m |= MOD_SHIFT;
    }
    if key_down(0x5B) || key_down(0x5C) {
        m |= MOD_WIN;
    }
    m
}

// Solo sondea con un juego detectado o el replay en marcha: fuera de eso RegisterHotKey basta y
// el hilo duerme. Cada vuelta son unas pocas llamadas baratas al estado del teclado.
#[cfg(target_os = "windows")]
pub fn spawn_poller(app: AppHandle) {
    let _ = std::thread::Builder::new().name("flashback-hotkey-poll".into()).spawn(move || {
        let mut down = [false; 3];
        loop {
            let gaming = crate::detect::current_game_pid().is_some() || crate::capture::replay_active();
            if PAUSED.load(Ordering::SeqCst) || !gaming {
                down = [false; 3];
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            let mods = mods_down();
            let mut fired = [None; 3];
            {
                let combos = COMBOS.lock().unwrap_or_else(|e| e.into_inner());
                for c in combos.iter() {
                    let i = c.action as usize;
                    let pressed = mods == c.mods && key_down(c.vk);
                    if pressed && !down[i] {
                        fired[i] = Some(c.action);
                    }
                    down[i] = pressed;
                }
            }
            for action in fired.into_iter().flatten() {
                trigger(&app, action, "sondeo");
            }
            std::thread::sleep(Duration::from_millis(12));
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_poller(_app: AppHandle) {}

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
    let mut combos = Vec::new();
    let es = spanish(app);
    for (accel, action) in [(&keys.save, Action::Save), (&keys.record, Action::Record), (&keys.open, Action::Open)] {
        if accel.is_empty() {
            continue;
        }
        combos.extend(parse_combo(action, accel));
        let r = app.global_shortcut().on_shortcut(accel.as_str(), move |app, _, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            trigger(app, action, "RegisterHotKey");
        });
        if let Err(e) = r {
            log::warn!("atajos: no se pudo registrar {accel}: {e}");
            let name = match (action, es) {
                (Action::Save, true) => "guardar clip",
                (Action::Save, false) => "save clip",
                (Action::Record, true) => "grabación",
                (Action::Record, false) => "recording",
                (Action::Open, true) => "abrir Flashback",
                (Action::Open, false) => "open Flashback",
            };
            failed.push((accel.clone(), format!("{name} ({})", key_labels(accel).join(" + "))));
        }
    }
    *COMBOS.lock().unwrap_or_else(|e| e.into_inner()) = combos;
    *FAILED.lock().unwrap_or_else(|e| e.into_inner()) = failed;
    show_conflicts(app);
    self::failed()
}

// Avisa de los atajos ocupados por otra app. Devuelve false si no hay ninguno. Lo repite el aviso
// de "listo para clipear" cuando el atajo de guardar falló: el toast no tiene cola y ese aviso
// tapaba este, así que solo se veía anunciar una combinación que no funciona.
pub fn show_conflicts(app: &AppHandle) -> bool {
    let failed = FAILED.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if failed.is_empty() {
        return false;
    }
    let list = failed.into_iter().map(|(_, name)| name).collect::<Vec<_>>().join(", ");
    let body = if spanish(app) {
        format!("Atajo en uso por otra app: {list}. Cámbialo en Ajustes.")
    } else {
        format!("Shortcut in use by another app: {list}. Change it in Settings.")
    };
    notify(app, ToastTopic::Problems, "error", body, Vec::new());
    true
}

pub fn save_failed(accel: &str) -> bool {
    FAILED.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|(a, _)| a == accel)
}

#[cfg(not(desktop))]
pub fn apply(_app: &AppHandle, _keys: &HotkeyPrefs) -> Vec<String> {
    Vec::new()
}

pub fn failed() -> Vec<String> {
    FAILED.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(a, _)| a.clone()).collect()
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
    use super::{key_labels, parse_combo, Action, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN};

    #[test]
    fn combos_parse_to_the_virtual_keys_windows_reports() {
        let c = parse_combo(Action::Save, "Alt+F8").expect("alt+f8");
        assert_eq!((c.mods, c.vk), (MOD_ALT, 0x77));
        let c = parse_combo(Action::Save, "Control+Numpad3").expect("ctrl+num3");
        assert_eq!((c.mods, c.vk), (MOD_CTRL, 0x63));
        let c = parse_combo(Action::Save, "Shift+NumpadAdd").expect("shift+num+");
        assert_eq!((c.mods, c.vk), (MOD_SHIFT, 0x6B));
        let c = parse_combo(Action::Save, "Super+Space").expect("win+space");
        assert_eq!((c.mods, c.vk), (MOD_WIN, 0x20));
        let c = parse_combo(Action::Save, "k").expect("k");
        assert_eq!((c.mods, c.vk), (0, 0x4B));
        let c = parse_combo(Action::Save, "Alt+;").expect("alt+;");
        assert_eq!((c.mods, c.vk), (MOD_ALT, 0xBA));
        assert!(parse_combo(Action::Save, "Alt").is_none());
        assert!(parse_combo(Action::Save, "Alt+Nope").is_none());
    }

    #[test]
    fn key_labels_match_the_interface() {
        assert_eq!(key_labels("Alt+F8"), ["ALT", "F8"]);
        assert_eq!(key_labels("Control+Numpad3"), ["CTRL", "Num 3"]);
        assert_eq!(key_labels("Shift+NumpadAdd"), ["SHIFT", "Num +"]);
        assert_eq!(key_labels("Super+Space"), ["WIN", "Espacio"]);
        assert_eq!(key_labels("k"), ["K"]);
    }
}
