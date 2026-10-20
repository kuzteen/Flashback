// Rich Presence de Discord (opt-in, off por defecto). Un hilo en segundo plano mantiene la
// conexión IPC con el cliente de Discord y refresca la presencia según el estado (grabando /
// Instant Replay / galería / ausente) y el juego detectado; el tiempo cuenta desde el juego actual o
// la vuelta a la biblioteca, no desde que se abrió la app. Aislado del camino de captura: solo lee
// estado cada pocos segundos y reconecta solo si Discord se cierra/abre.

use std::collections::HashMap;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, Assets, Button, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

const APP_ID: &str = "1495797767245922498";
// PNG público del mismo isotipo que usa la app; evita mantener otro logo en Discord Art Assets.
const FLASHBACK_ASSET: &str =
    "https://raw.githubusercontent.com/kuzteen/Flashback/main/static/flashback-presence.png";
const DOWNLOAD_URL: &str = "https://github.com/kuzteen/Flashback/releases/latest";
const SUPPORT_URL: &str = "https://discord.gg/WckwXRrhrR";
const AWAY_AFTER: Duration = Duration::from_secs(60);

// Solo importa la actividad de Flashback: usar otra app no mantiene esta presencia activa.
// El reloj vive en Rust para seguir funcionando con el webview descargado en la bandeja.
struct Inactivity {
    last_active: Instant,
}

impl Inactivity {
    fn new(now: Instant) -> Self {
        Self { last_active: now }
    }

    fn update(&mut self, now: Instant, focused: bool, capturing: bool) -> bool {
        if focused || capturing {
            self.last_active = now;
            return false;
        }
        now.saturating_duration_since(self.last_active) >= AWAY_AFTER
    }
}

fn main_focused() -> bool {
    use tauri::Manager;
    APP.get()
        .and_then(|app| app.get_webview_window("main"))
        .is_some_and(|window| window.is_focused().unwrap_or(true))
}

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
    details: Option<&'static str>,
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
    let mut inactivity = Inactivity::new(Instant::now());
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
            inactivity = Inactivity::new(Instant::now());
            continue;
        }

        // Usa el estado real de captura, no el toggle del replay: puede estar armado esperando
        // un juego sin estar clipeando. Se actualiza incluso si Discord está desconectado.
        let recording = crate::capture::status().running;
        let replay = crate::capture::replay_active();
        let capturing = recording || replay;
        let away = inactivity.update(Instant::now(), !capturing && main_focused(), capturing);

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

        let p = presence(&mut art_cache, recording, replay, away);
        if p.game != last_game {
            last_game = p.game.clone();
            started = now_ms();
        }
        // Solo se reenvía a Discord si algo cambió (evita spam de set_activity).
        let key = format!(
            "{}\u{1}{}\u{1}{}\u{1}{}\u{1}{started}",
            p.name, p.details.unwrap_or(""), p.large_image, p.large_text
        );
        if key != last_key {
            if let Some(c) = client.as_mut() {
                let mut assets = Assets::new()
                    .large_image(p.large_image.as_str())
                    .large_text(p.large_text.as_str());
                if p.game.is_some() {
                    assets = assets.small_image(FLASHBACK_ASSET).small_text("Flashback");
                }
                let mut act = Activity::new()
                    .name(p.name.as_str())
                    .assets(assets)
                    .buttons(vec![
                        Button::new(p.buttons[0], DOWNLOAD_URL),
                        Button::new(p.buttons[1], SUPPORT_URL),
                    ])
                    .timestamps(Timestamps::new().start(started));
                if let Some(details) = p.details {
                    act = act.details(details);
                }
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
fn presence(art_cache: &mut HashMap<String, String>, recording: bool, replay: bool, away: bool) -> Presence {
    let es = APP
        .get()
        .map(|app| crate::config::get_language(app) == "es")
        .unwrap_or(false);
    let buttons = if es {
        ["Descargar Flashback", "Soporte"]
    } else {
        ["Get Flashback", "Need help?"]
    };
    if recording || replay {
        if let Some(g) = crate::detect::current_game() {
            return Presence {
                name: if es {
                    format!("{} con Flashback", g.name)
                } else {
                    format!("{} with Flashback", g.name)
                },
                details: None,
                large_image: art_url(art_cache, &g),
                large_text: g.name.clone(),
                game: Some(g.name),
                buttons,
            };
        }
    }

    Presence {
        game: None,
        name: "Flashback".to_string(),
        details: Some(match (away, es) {
            (true, true) => "Ausente",
            (true, false) => "Away",
            (false, true) => "En la galería",
            (false, false) => "In the gallery",
        }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_waits_one_minute_before_away() {
        let now = Instant::now();
        let mut idle = Inactivity::new(now);
        assert!(!idle.update(now + Duration::from_secs(59), false, false));
        assert!(idle.update(now + AWAY_AFTER, false, false));
        assert!(idle.update(now + Duration::from_secs(120), false, false));
    }

    #[test]
    fn returning_to_flashback_clears_away_and_restarts_the_grace_period() {
        let now = Instant::now();
        let mut idle = Inactivity::new(now);
        let later = now + Duration::from_secs(120);
        assert!(idle.update(later, false, false));
        assert!(!idle.update(later, true, false));
        assert!(!idle.update(later + Duration::from_secs(59), false, false));
        assert!(idle.update(later + AWAY_AFTER, false, false));
    }

    #[test]
    fn active_capture_never_goes_away_and_gets_a_grace_period_after_stopping() {
        let now = Instant::now();
        let mut idle = Inactivity::new(now);
        let later = now + Duration::from_secs(300);
        assert!(!idle.update(later, false, true));
        assert!(!idle.update(later + Duration::from_secs(59), false, false));
        assert!(idle.update(later + AWAY_AFTER, false, false));
    }

    #[test]
    fn starting_capture_clears_an_existing_away_state() {
        let now = Instant::now();
        let mut idle = Inactivity::new(now);
        let later = now + AWAY_AFTER;
        assert!(idle.update(later, false, false));
        assert!(!idle.update(later, false, true));
    }
}
