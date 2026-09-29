// Rich Presence de Discord (opt-in, off por defecto). Un hilo en segundo plano mantiene la
// conexión IPC con el cliente de Discord y refresca la presencia según el estado (grabando /
// Instant Replay / biblioteca) y el juego detectado; el tiempo cuenta desde el juego actual o
// la vuelta a la biblioteca, no desde que se abrió la app. Aislado del camino de captura: solo lee
// estado cada pocos segundos y reconecta solo si Discord se cierra/abre.

use std::collections::HashMap;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, Assets, Button, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

const APP_ID: &str = "1495797767245922498";
// Asset subido en la app de Discord (Rich Presence → Art Assets) con el logo de Flashback.
const FLASHBACK_ASSET: &str = "flashback";
const DOWNLOAD_URL: &str = "https://github.com/kuzteen/Flashback/releases/latest";
const SUPPORT_URL: &str = "https://discord.gg/WckwXRrhrR";

struct Shared {
    enabled: bool,
}

fn state() -> &'static (Mutex<Shared>, Condvar) {
    static STATE: OnceLock<(Mutex<Shared>, Condvar)> = OnceLock::new();
    STATE.get_or_init(|| (Mutex::new(Shared { enabled: false }), Condvar::new()))
}

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

// Arranca el gestor una sola vez con el valor persistido. Idempotente.
pub fn init(app: tauri::AppHandle, enabled: bool) {
    let _ = APP.set(app);
    set_enabled(enabled);
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_ok() {
        let _ = std::thread::Builder::new()
            .name("flashback-discord".into())
            .spawn(run);
    }
}

pub fn set_enabled(enabled: bool) {
    let (m, cv) = state();
    m.lock().unwrap().enabled = enabled;
    cv.notify_all();
}

fn is_enabled() -> bool {
    state().0.lock().unwrap().enabled
}

// Espera hasta `dur` o hasta que cambie el toggle (notify), para reaccionar rápido a on/off.
fn wait(dur: Duration) {
    let (m, cv) = state();
    if let Ok(g) = m.lock() {
        let _ = cv.wait_timeout(g, dur);
    }
}

struct Presence {
    // Juego que se está capturando; None es la biblioteca. Al cambiar se reinicia el tiempo.
    game: Option<String>,
    name: String,
    details: String,
    large_image: String,
    large_text: String,
    buttons: [&'static str; 2],
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn run() {
    let mut client: Option<DiscordIpcClient> = None;
    let mut last_key = String::new();
    let mut last_game: Option<String> = None;
    let mut started = now_ms();
    // Arte resuelto por nombre de juego: una sola llamada HTTP por juego (URL pública o el
    // asset de fallback). Se puebla perezosamente en presence.
    let mut art_cache: HashMap<String, String> = HashMap::new();

    loop {
        if !is_enabled() {
            if let Some(mut c) = client.take() {
                let _ = c.close();
            }
            last_key.clear();
            let (m, cv) = state();
            if let Ok(g) = m.lock() {
                let _ = cv.wait_timeout_while(g, Duration::from_secs(60), |s| !s.enabled);
            }
            continue;
        }

        // Asegurar conexión: si Discord no está abierto, reintentar en 15 s.
        if client.is_none() {
            let mut c = DiscordIpcClient::new(APP_ID);
            if c.connect().is_ok() {
                client = Some(c);
                last_key.clear();
            } else {
                wait(Duration::from_secs(15));
                continue;
            }
        }

        let p = presence(&mut art_cache);
        if p.game != last_game {
            last_game = p.game.clone();
            started = now_ms();
        }
        // Solo se reenvía a Discord si algo cambió (evita spam de set_activity).
        let key = format!(
            "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{started}",
            p.name, p.details, p.large_image, p.large_text
        );
        if key != last_key {
            if let Some(c) = client.as_mut() {
                let mut assets = Assets::new()
                    .large_image(p.large_image.as_str())
                    .large_text(p.large_text.as_str());
                if p.game.is_some() {
                    assets = assets.small_image(FLASHBACK_ASSET).small_text("Flashback");
                }
                let act = Activity::new()
                    .name(p.name.as_str())
                    .details(p.details.as_str())
                    .assets(assets)
                    .buttons(vec![
                        Button::new(p.buttons[0], DOWNLOAD_URL),
                        Button::new(p.buttons[1], SUPPORT_URL),
                    ])
                    .timestamps(Timestamps::new().start(started));
                if c.set_activity(act).is_err() {
                    // Conexión caída (Discord cerrado): reconectar en el próximo ciclo.
                    let _ = c.close();
                    client = None;
                    last_key.clear();
                    wait(Duration::from_secs(5));
                    continue;
                }
                last_key = key;
            }
        }

        wait(Duration::from_secs(5));
    }
}

// Con un juego capturándose, la tarjeta es del juego ("Valorant con Flashback", su arte en
// grande y el logo en pequeño); si no, la de Flashback con el estado de la app.
fn presence(art_cache: &mut HashMap<String, String>) -> Presence {
    let es = APP
        .get()
        .map(|app| crate::config::get_language(app) == "es")
        .unwrap_or(false);
    let buttons = if es {
        ["Descargar Flashback", "Soporte"]
    } else {
        ["Get Flashback", "Need help?"]
    };
    let recording = crate::capture::status().running;
    let replay = crate::capture::replay_active();

    if recording || replay {
        if let Some(g) = crate::detect::current_game() {
            let details = match (recording, es) {
                (true, true) => "Grabando la partida",
                (true, false) => "Recording gameplay",
                (false, true) => "Clipeando cada jugada",
                (false, false) => "Clipping every play",
            };
            return Presence {
                name: if es {
                    format!("{} con Flashback", g.name)
                } else {
                    format!("{} with Flashback", g.name)
                },
                details: details.to_string(),
                large_image: art_url(art_cache, &g),
                large_text: g.name.clone(),
                game: Some(g.name),
                buttons,
            };
        }
    }

    let details = match (recording, replay, es) {
        (true, _, true) => "Grabando",
        (true, _, false) => "Recording",
        (false, true, true) => "Instant Replay activo",
        (false, true, false) => "Instant Replay active",
        (false, false, true) => "En la biblioteca",
        (false, false, false) => "In the library",
    };
    Presence {
        game: None,
        name: "Flashback".to_string(),
        details: details.to_string(),
        large_image: FLASHBACK_ASSET.to_string(),
        large_text: "Flashback".to_string(),
        buttons,
    }
}

// Imagen grande del juego. Prioridad: icono nativo del CDN de Discord (fiable, directo) si la
// detección ya lo traía; si no, game_art_url lo busca por nombre en la misma lista y luego en
// SteamGridDB (cacheado por nombre). Si nada se puede resolver, cae al asset del logo.
fn art_url(cache: &mut HashMap<String, String>, g: &crate::detect::DetectedGame) -> String {
    if let Some(url) = &g.icon_url {
        return url.clone();
    }
    if let Some(url) = cache.get(&g.name) {
        return url.clone();
    }
    let resolved = APP
        .get()
        .and_then(|app| {
            tauri::async_runtime::block_on(crate::artwork::game_art_url(app, &g.name, g.steam_appid))
        })
        .unwrap_or_else(|| FLASHBACK_ASSET.to_string());
    cache.insert(g.name.clone(), resolved.clone());
    resolved
}
