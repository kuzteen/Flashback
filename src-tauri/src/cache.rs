use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::Manager;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    EditorAudio,
    Share,
    Thumbnails,
    Artwork,
    Webview,
}

#[derive(Serialize)]
pub struct Usage {
    pub kind: Kind,
    pub bytes: u64,
}

// Solo lo que se regenera solo al volver a usarse. Quedan fuera los datos del usuario (ajustes,
// playlists, cortes, metadatos y portadas elegidas) y la base de datos de juegos de Discord: sin
// ella, un arranque sin red se quedaría sin detección de juegos.
fn dirs(app: &tauri::AppHandle) -> Vec<(Kind, PathBuf)> {
    let path = app.path();
    let mut out = Vec::new();
    let data = path.app_data_dir().ok();
    if let Some(data) = &data {
        out.push((Kind::EditorAudio, data.join("audio")));
    }
    out.push((Kind::Share, crate::share::dir(app)));
    if let Some(data) = &data {
        out.push((Kind::Thumbnails, data.join("thumbs")));
    }
    if let Ok(cache) = path.app_cache_dir() {
        out.push((Kind::Artwork, cache.join("artwork")));
        out.push((Kind::Artwork, cache.join("artwork-search")));
    }
    // El WebView2 no se vacía borrando carpetas (las tiene abiertas y comparten perfil con el
    // localStorage de los ajustes); estas rutas solo sirven para medirlo.
    if let Ok(local) = path.app_local_data_dir() {
        let profile = local.join("EBWebView").join("Default");
        out.push((Kind::Webview, profile.join("Cache")));
        out.push((Kind::Webview, profile.join("Code Cache")));
    }
    out
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

// Borra el contenido y deja la carpeta: quien la usa asume que existe. Un archivo en uso (un
// compartido que se está arrastrando) no se puede borrar en Windows y simplemente se queda.
fn empty_dir(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let _ = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
    }
}

pub fn usage(app: &tauri::AppHandle) -> Vec<Usage> {
    let mut out: Vec<Usage> = Vec::new();
    for (kind, dir) in dirs(app) {
        let bytes = dir_size(&dir);
        match out.iter_mut().find(|u| u.kind == kind) {
            Some(u) => u.bytes += bytes,
            None => out.push(Usage { kind, bytes }),
        }
    }
    out
}

pub async fn clear(app: &tauri::AppHandle) -> Result<(), String> {
    let targets: Vec<PathBuf> = dirs(app)
        .into_iter()
        .filter(|(k, _)| *k != Kind::Webview)
        .map(|(_, d)| d)
        .collect();
    tokio::task::spawn_blocking(move || targets.iter().for_each(|d| empty_dir(d)))
        .await
        .map_err(|e| e.to_string())?;
    clear_webview(app).await
}

#[cfg(target_os = "windows")]
async fn clear_webview(app: &tauri::AppHandle) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Profile2, ICoreWebView2_13, COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE,
    };
    use webview2_com::ClearBrowsingDataCompletedHandler;
    use windows::core::Interface;

    let window = app.get_webview_window("main").ok_or("No hay ventana principal")?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let fail = tx.clone();
    window
        .with_webview(move |wv| {
            let started = (|| -> windows::core::Result<()> {
                unsafe {
                    let core: ICoreWebView2_13 = wv.controller().CoreWebView2()?.cast()?;
                    let profile: ICoreWebView2Profile2 = core.Profile()?.cast()?;
                    let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |r| {
                        let _ = tx.send(r.map_err(|e| e.to_string()));
                        Ok(())
                    }));
                    profile.ClearBrowsingData(COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE, &handler)
                }
            })();
            if let Err(e) = started {
                let _ = fail.send(Err(e.to_string()));
            }
        })
        .map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| "El WebView no respondió al vaciar su caché".to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(target_os = "windows"))]
async fn clear_webview(_app: &tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

// El audio que el editor separa por pista son WAV sin comprimir (cientos de MB por clip) y nadie
// los borraba. Al reutilizarse se les renueva la fecha, así que solo caen los de clips que no se
// han abierto en el editor desde hace `max_age`.
pub fn prune_editor_audio(app: &tauri::AppHandle, max_age: Duration) {
    let Ok(dir) = app.path().app_data_dir().map(|d| d.join("audio")) else {
        return;
    };
    prune_older_than(&dir, max_age);
}

fn prune_older_than(dir: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > max_age);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

pub fn touch(path: &str) {
    let _ = std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(SystemTime::now()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("flashback-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn size_counts_nested_files_and_empty_keeps_the_folder() {
        let dir = scratch("size");
        std::fs::write(dir.join("a"), [0u8; 10]).unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("b"), [0u8; 5]).unwrap();
        assert_eq!(dir_size(&dir), 15);
        empty_dir(&dir);
        assert!(dir.is_dir());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_folder_is_zero_and_harmless() {
        let dir = std::env::temp_dir().join("flashback-cache-no-existe");
        assert_eq!(dir_size(&dir), 0);
        empty_dir(&dir);
        prune_older_than(&dir, Duration::from_secs(1));
    }

    #[test]
    fn prune_drops_old_files_and_touch_saves_them() {
        let dir = scratch("prune");
        let old = dir.join("old.wav");
        let reused = dir.join("reused.wav");
        let fresh = dir.join("fresh.wav");
        for p in [&old, &reused, &fresh] {
            std::fs::write(p, b"x").unwrap();
        }
        let week_ago = SystemTime::now() - Duration::from_secs(8 * 24 * 3600);
        for p in [&old, &reused] {
            std::fs::File::options().write(true).open(p).unwrap().set_modified(week_ago).unwrap();
        }
        touch(reused.to_str().unwrap());
        prune_older_than(&dir, Duration::from_secs(7 * 24 * 3600));
        assert!(!old.exists());
        assert!(reused.exists());
        assert!(fresh.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
