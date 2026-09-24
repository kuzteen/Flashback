mod artwork;
#[cfg(target_os = "windows")]
mod audio;
mod capture;
mod config;
mod detect;
mod discord;
mod dragdrop;
mod editor;
mod hotkeys;
mod edits;
mod library;
mod look;
mod mp4mux;
mod playlists;
mod reframe;
mod share;
#[cfg(target_os = "windows")]
mod overlay;
mod thumbnail;
#[cfg(target_os = "windows")]
mod toast;
mod watermark;

#[tauri::command]
async fn game_hero(
    app: tauri::AppHandle,
    name: String,
    steam_appid: Option<u32>,
) -> Option<String> {
    artwork::game_hero(&app, &name, steam_appid).await
}

#[tauri::command]
async fn game_icon(
    app: tauri::AppHandle,
    name: String,
    steam_appid: Option<u32>,
) -> Option<String> {
    artwork::game_icon(&app, &name, steam_appid).await
}

#[tauri::command]
async fn detect_game(app: tauri::AppHandle) -> Option<detect::DetectedGame> {
    detect::detect_game(&app).await
}

#[tauri::command]
fn get_seen_games(app: tauri::AppHandle) -> Vec<config::SeenGame> {
    config::get_seen_games(&app)
}

#[tauri::command]
fn get_disabled_games(app: tauri::AppHandle) -> Vec<String> {
    config::get_disabled_games(&app)
}

#[tauri::command]
fn set_disabled_games(app: tauri::AppHandle, games: Vec<String>) -> Result<(), String> {
    config::set_disabled_games(&app, games)
}

#[tauri::command]
fn list_monitors() -> Vec<capture::MonitorInfo> {
    capture::list_monitors()
}

#[tauri::command]
fn list_audio_inputs() -> Vec<capture::AudioInput> {
    capture::list_audio_inputs()
}

#[tauri::command]
fn get_encoder(app: tauri::AppHandle) -> String {
    config::get_encoder(&app)
}

#[tauri::command]
fn set_encoder(app: tauri::AppHandle, enc: String) -> Result<(), String> {
    config::set_encoder(&app, &enc)
}

#[tauri::command]
fn get_discord_rpc(app: tauri::AppHandle) -> bool {
    config::get_discord_rpc(&app)
}

#[tauri::command]
fn set_discord_rpc(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    config::set_discord_rpc(&app, enabled)?;
    discord::set_enabled(enabled);
    Ok(())
}

#[tauri::command]
fn get_language(app: tauri::AppHandle) -> String {
    config::get_language(&app)
}

#[tauri::command]
fn set_language(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    config::set_language(&app, &lang)
}

#[tauri::command]
fn get_watermark(app: tauri::AppHandle) -> bool {
    config::get_watermark(&app)
}

#[tauri::command]
fn set_watermark(app: tauri::AppHandle, on: bool) -> Result<(), String> {
    config::set_watermark(&app, on)
}

#[tauri::command]
fn get_watermark_corner(app: tauri::AppHandle) -> String {
    config::get_watermark_corner(&app)
}

#[tauri::command]
fn set_watermark_corner(app: tauri::AppHandle, corner: String) -> Result<(), String> {
    config::set_watermark_corner(&app, &corner)
}

// Los comandos de captura van fuera del hilo principal: un comando síncrono de Tauri corre en él,
// y construir el pipeline, pararlo o muxear un replay de minutos congelaba la ventana (y retenía
// los atajos globales, que también llegan por ese hilo) mientras duraba.
async fn off_main<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| format!("Error interno: {e}"))
}

#[tauri::command]
async fn start_capture(
    app: tauri::AppHandle,
    target: String,
    fps: u32,
    quality: String,
    resolution: u32,
    bitrate: u32,
    mic: bool,
    mic_device: String,
) -> Result<bool, String> {
    let dir = config::clips_dir(&app).to_string_lossy().into_owned();
    let encoder_pref = config::get_encoder(&app);
    off_main(move || {
        capture::start(target, dir, fps, quality, resolution, bitrate, mic, mic_device, encoder_pref)
    })
    .await?
}

#[tauri::command]
async fn stop_capture() -> Option<String> {
    off_main(capture::stop).await.ok().flatten()
}

#[tauri::command]
fn capture_status() -> capture::CaptureStatus {
    capture::status()
}

#[tauri::command]
async fn start_replay(
    app: tauri::AppHandle,
    target: String,
    seconds: u32,
    fps: u32,
    quality: String,
    resolution: u32,
    bitrate: u32,
    mic: bool,
    mic_device: String,
) -> Result<(), String> {
    let dir = config::clips_dir(&app).to_string_lossy().into_owned();
    let encoder_pref = config::get_encoder(&app);
    let app_ev = app.clone();
    let on_retarget = Box::new(move || {
        use tauri::Emitter;
        let _ = app_ev.emit("replay-retargeted", ());
    });
    let card_text = match config::get_language(&app).as_str() {
        "es" => "Aquí estaremos cuando vuelvas",
        _ => "We'll be here when you're back",
    }
    .to_string();
    off_main(move || {
        capture::start_replay(
            target,
            dir,
            seconds,
            fps,
            quality,
            resolution,
            bitrate,
            mic,
            mic_device,
            encoder_pref,
            on_retarget,
            card_text,
        )
    })
    .await?
}

// Atajos de guardar y grabar: los atiende Rust directamente (ver hotkeys.rs). Asíncrono porque el
// registro del plugin pasa por el hilo principal y espera su respuesta.
#[tauri::command]
async fn set_native_hotkeys(app: tauri::AppHandle, save: String, record: String) -> Vec<String> {
    hotkeys::register(&app, &save, &record)
}

#[tauri::command]
fn set_record_prefs(prefs: hotkeys::RecordPrefs) {
    hotkeys::set_record_prefs(prefs);
}

#[tauri::command]
fn set_save_sound(gain: f32) {
    hotkeys::set_sound_gain(gain);
}

#[tauri::command]
async fn stop_replay() {
    let _ = off_main(capture::stop_replay).await;
}

#[tauri::command]
async fn save_replay(source: String) -> Option<String> {
    off_main(move || capture::save_replay(&source)).await.ok().flatten()
}

#[tauri::command]
fn replay_active() -> bool {
    capture::replay_active()
}

#[tauri::command]
async fn prepare_clip_audio(app: tauri::AppHandle, path: String) -> Result<editor::ClipAudio, String> {
    use tauri::Manager;
    let audio_dir = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("audio");
    std::fs::create_dir_all(&audio_dir).map_err(|e| e.to_string())?;
    let audio_dir = audio_dir.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || editor::prepare_clip_audio(path, audio_dir))
        .await
        .map_err(|e| format!("Error interno: {e}"))?
}

fn edit_index(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("edits.json"))
}

#[tauri::command]
fn load_clip_edit(app: tauri::AppHandle, path: String) -> Result<editor::ClipEdit, String> {
    editor::load_edit(edit_index(&app)?.to_string_lossy().into_owned(), path)
}

#[tauri::command]
fn clips_with_edits(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    Ok(editor::edited_paths(&edit_index(&app)?))
}

#[tauri::command]
fn save_clip_edit(app: tauri::AppHandle, path: String, edit: editor::ClipEdit) -> Result<(), String> {
    editor::save_edit(edit_index(&app)?.to_string_lossy().into_owned(), path, edit)
}

#[tauri::command]
async fn keyframe_times(path: String) -> Result<Vec<f64>, String> {
    tokio::task::spawn_blocking(move || editor::keyframe_times(path))
        .await
        .map_err(|e| format!("Error interno: {e}"))?
}

#[tauri::command]
async fn frame_times(path: String) -> Result<Vec<f64>, String> {
    tokio::task::spawn_blocking(move || editor::frame_times(path))
        .await
        .map_err(|e| format!("Error interno: {e}"))?
}

#[tauri::command]
async fn clip_fps(path: String) -> Result<u32, String> {
    tokio::task::spawn_blocking(move || editor::clip_fps(path))
        .await
        .map_err(|e| format!("Error interno: {e}"))?
}

#[tauri::command]
async fn export_clip(
    app: tauri::AppHandle,
    src: String,
    dst: String,
    edit: editor::ClipEdit,
) -> Result<(), String> {
    use tauri::Emitter;
    // Marca de agua: solo si está activada. Se hornea únicamente aquí (export), nunca en captura.
    let watermark = config::get_watermark(&app).then(|| config::get_watermark_corner(&app));
    tokio::task::spawn_blocking(move || {
        editor::export_clip(src, dst, edit, watermark, None, None, None, move |p: f32| {
            let _ = app.emit("export-progress", p);
        })
    })
    .await
    .map_err(|e| format!("Error interno: {e}"))?
}

// Decide qué archivo se arrastra al compartir. El camino rápido —clip sin cortes, sin preset de
// tamaño y sin marca de agua— devuelve el original sin tocar nada, que es el caso habitual desde la
// biblioteca. Lo demás se materializa en un temporal cacheado, siempre por el export (§2: el único
// camino que recodifica).
#[tauri::command]
async fn share_prepare(
    app: tauri::AppHandle,
    src: String,
    edit: editor::ClipEdit,
    target_bytes: Option<u64>,
    watermark: bool,
) -> Result<String, String> {
    use tauri::Emitter;
    let path = std::path::PathBuf::from(&src);
    if !path.is_file() {
        return Err("El archivo ya no existe".into());
    }
    let duration = library::clip_duration_secs(&path).unwrap_or(0.0);
    let corner = watermark.then(|| config::get_watermark_corner(&app));

    if target_bytes.is_none() && corner.is_none() && share::shares_original(&path, &edit, duration) {
        return Ok(src);
    }

    let dir = share::dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dst = dir.join(share::cache_name(&path, &edit, target_bytes, corner.as_deref()));
    if dst.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(dst.to_string_lossy().into_owned());
    }

    let (bitrate, max_height) = match target_bytes {
        Some(bytes) => {
            let (w, h, fps, src_bps) = editor::clip_dims(src.clone())?;
            // El presupuesto se reparte sobre lo que realmente se conserva, no sobre el clip entero:
            // si hay cortes, el material a codificar es más corto y le toca más bitrate.
            let kept: f64 = edit
                .segments
                .iter()
                .filter(|s| !s.disabled.unwrap_or(false))
                .map(|s| (s.end_ms - s.start_ms).max(0.0) / 1000.0)
                .sum();
            let t = share::plan(bytes, if kept > 0.0 { kept } else { duration }, w, h, fps, src_bps);
            (Some(t.bitrate), t.max_height)
        }
        None => (None, None),
    };

    let dst_str = dst.to_string_lossy().into_owned();
    let out = dst_str.clone();
    let cancel = share::begin_job();
    tokio::task::spawn_blocking(move || {
        editor::export_clip(src, dst_str, edit, corner, bitrate, max_height, Some(cancel), move |p: f32| {
            let _ = app.emit("share-progress", p);
        })
    })
    .await
    .map_err(|e| format!("Error interno: {e}"))??;
    Ok(out)
}

#[tauri::command]
fn share_cancel() {
    share::cancel_current();
}

// SHDoDragDrop es modal y exige STA con OLE inicializado y la captura del ratón, así que solo
// funciona en el hilo del bucle de eventos; los comandos de Tauri corren en el runtime async (MTA).
// Bloquea ese hilo mientras dura el arrastre —igual que el Explorador—, pero la captura, el encoder
// y el audio viven en sus propios hilos y siguen corriendo sin enterarse.
#[tauri::command]
async fn start_file_drag(
    app: tauri::AppHandle,
    path: String,
    thumb_src: Option<String>,
) -> Result<bool, String> {
    use tauri::Manager;
    if !std::path::Path::new(&path).is_file() {
        return Err("El archivo ya no existe".into());
    }
    let hwnd = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as isize)
        .unwrap_or(0);
    // La miniatura es la del clip de origen, no la del archivo que se arrastra: con un preset de
    // tamaño ese es un temporal recodificado que no tiene miniatura propia.
    let thumb = thumb_src
        .and_then(|c| thumb_path_for(&app, &c))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned());
    // Canal asíncrono a propósito: el arrastre dura lo que el usuario tarde en soltar, y esperarlo
    // con un recv() bloqueante dejaría atascado un worker de tokio (y con él otros comandos).
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(dragdrop::drag(hwnd, &path, thumb.as_deref()));
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|e| e.to_string())?
}

// Ruta de la miniatura cacheada de un clip. La comparten el comando que la genera y el arrastre,
// que la reutiliza como imagen bajo el cursor: si los dos no derivan el nombre igual, el arrastre
// se quedaría sin imagen aunque la miniatura ya exista. Tamaño y fecha entran en el nombre: un
// archivo nuevo en la ruta de uno borrado (otro export con el mismo nombre) no hereda su miniatura.
fn thumb_path_for(app: &tauri::AppHandle, clip: &str) -> Option<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    use tauri::Manager;
    let dir = app.path().app_data_dir().ok()?.join("thumbs");
    let mut h = std::collections::hash_map::DefaultHasher::new();
    clip.hash(&mut h);
    if let Ok(m) = std::fs::metadata(clip) {
        m.len().hash(&mut h);
        m.modified().ok().hash(&mut h);
    }
    Some(dir.join(format!("{:016x}.jpg", h.finish())))
}

#[tauri::command]
async fn clip_thumbnail(app: tauri::AppHandle, path: String) -> Result<String, String> {
    let dst = thumb_path_for(&app, &path).ok_or("No hay carpeta de datos")?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let thumb_path = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let ready = dst.metadata().map(|m| m.len() > 0).unwrap_or(false);
        if !ready {
            thumbnail::generate(path, dst.to_string_lossy().into_owned(), 0)?;
        }
        Ok(dst.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("Error interno: {e}"))?;
    thumb_path
}

// Sin caché en disco: generarla cuesta una fracción de segundo y el frontend conserva las de los
// últimos clips abiertos durante la sesión.
#[tauri::command]
async fn clip_filmstrip(path: String) -> Result<tauri::ipc::Response, String> {
    let strip = tokio::task::spawn_blocking(move || thumbnail::filmstrip(path, 72, 180))
        .await
        .map_err(|e| format!("Error interno: {e}"))??;
    Ok(tauri::ipc::Response::new(strip.into_bytes()))
}

#[tauri::command]
async fn capture_frame(app: tauri::AppHandle, path: String, time_ms: f64) -> Result<String, String> {
    let dir = config::screenshots_dir(&app);
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let stem = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("clip");
        let dst = dir
            .join(format!("{stem}_{}.png", time_ms.max(0.0).round() as i64))
            .to_string_lossy()
            .into_owned();
        thumbnail::capture(path.clone(), dst.clone(), time_ms)?;
        Ok(dst)
    })
    .await
    .map_err(|e| format!("Error interno: {e}"))?
}

#[tauri::command]
fn rename_clip(app: tauri::AppHandle, path: String, new_name: String) -> Result<String, String> {
    let new_path = library::rename_clip(&path, &new_name, &edit_index(&app)?)?;
    playlists::rekey(&playlist_index(&app)?, &path, &new_path);
    Ok(new_path)
}

#[tauri::command]
fn delete_clip(app: tauri::AppHandle, path: String) -> Result<(), String> {
    delete_clips(app, vec![path])
}

#[tauri::command]
fn delete_clips(app: tauri::AppHandle, paths: Vec<String>) -> Result<(), String> {
    library::delete_clips(&paths, &edit_index(&app)?)?;
    playlists::forget(&playlist_index(&app)?, &paths);
    Ok(())
}

fn playlist_index(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("playlists.json"))
}

#[tauri::command]
fn list_playlists(app: tauri::AppHandle) -> Result<Vec<playlists::Playlist>, String> {
    Ok(playlists::list(&playlist_index(&app)?))
}

#[tauri::command]
fn create_playlist(
    app: tauri::AppHandle,
    name: String,
    description: String,
) -> Result<playlists::Playlist, String> {
    playlists::create(&playlist_index(&app)?, &name, &description)
}

#[tauri::command]
fn update_playlist(
    app: tauri::AppHandle,
    id: String,
    name: String,
    description: String,
) -> Result<(), String> {
    playlists::update(&playlist_index(&app)?, &id, &name, &description)
}

fn playlist_covers_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("playlist-covers"))
}

#[tauri::command]
// El PNG llega como cuerpo binario de la petición y el id en una cabecera: como argumento
// Vec<u8> viajaba serializado en JSON como un array de números, uno por byte.
fn set_playlist_cover(app: tauri::AppHandle, request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("La portada tiene que llegar como bytes".into());
    };
    let id = request
        .headers()
        .get("x-playlist-id")
        .and_then(|v| v.to_str().ok())
        .ok_or("Falta el id de la playlist")?;
    playlists::set_cover(&playlist_index(&app)?, &playlist_covers_dir(&app)?, id, bytes)
}

#[tauri::command]
fn clear_playlist_cover(app: tauri::AppHandle, id: String) -> Result<(), String> {
    playlists::clear_cover(&playlist_index(&app)?, &id)
}

#[tauri::command]
fn reorder_playlists(app: tauri::AppHandle, ids: Vec<String>) -> Result<(), String> {
    playlists::reorder(&playlist_index(&app)?, &ids)
}

#[tauri::command]
fn delete_playlist(app: tauri::AppHandle, id: String) -> Result<(), String> {
    playlists::remove(&playlist_index(&app)?, &id)
}

#[tauri::command]
fn playlist_add_clips(app: tauri::AppHandle, id: String, paths: Vec<String>) -> Result<(), String> {
    playlists::add_clips(&playlist_index(&app)?, &id, &paths)
}

#[tauri::command]
fn playlist_remove_clips(app: tauri::AppHandle, id: String, paths: Vec<String>) -> Result<(), String> {
    playlists::remove_clips(&playlist_index(&app)?, &id, &paths)
}

#[tauri::command]
fn playlist_restore_clips(
    app: tauri::AppHandle,
    id: String,
    entries: Vec<(usize, playlists::ClipRef)>,
) -> Result<(), String> {
    playlists::restore_clips(&playlist_index(&app)?, &id, entries)
}

#[tauri::command]
fn playlist_set_clips(app: tauri::AppHandle, id: String, paths: Vec<String>) -> Result<(), String> {
    playlists::set_clips(&playlist_index(&app)?, &id, paths)
}

#[tauri::command]
fn playlist_mark_seen(app: tauri::AppHandle, id: String, path: String) -> Result<(), String> {
    playlists::mark_seen(&playlist_index(&app)?, &id, &path)
}

#[tauri::command]
fn list_clips(app: tauri::AppHandle) -> Vec<library::ClipInfo> {
    let clips = library::list_clips(config::library_dirs(&app));
    if !clips.is_empty() {
        let paths: Vec<String> = clips.iter().map(|c| c.path.clone()).collect();
        std::thread::spawn(move || prune_thumbs(&app, &paths));
    }
    clips
}

// Borra las miniaturas de clips que ya no existen o que cambiaron (su nombre ya no coincide).
fn prune_thumbs(app: &tauri::AppHandle, clips: &[String]) {
    let keep: std::collections::HashSet<std::path::PathBuf> =
        clips.iter().filter_map(|c| thumb_path_for(app, c)).collect();
    let Some(dir) = keep.iter().next().and_then(|p| p.parent()).map(|d| d.to_path_buf()) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "jpg") && !keep.contains(&p) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

#[tauri::command]
fn clips_dir(app: tauri::AppHandle) -> String {
    config::clips_dir(&app).to_string_lossy().into_owned()
}

#[tauri::command]
fn set_clips_dir(app: tauri::AppHandle, dir: String) -> Result<(), String> {
    config::set_clips_dir(&app, &dir)
}

#[tauri::command]
async fn pick_folder() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(config::pick_folder)
        .await
        .map_err(|e| format!("Error interno: {e}"))?
}

// Destino de exportación: los clips editados van a su carpeta dedicada, no junto al original.
#[tauri::command]
fn edit_dest(app: tauri::AppHandle, src: String) -> String {
    let stem = std::path::Path::new(&src)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("clip");
    free_path(&config::clips_edit_dir(&app), &format!("{stem}_edit"))
        .to_string_lossy()
        .into_owned()
}

// Primer nombre libre (`x.mp4`, `x_2.mp4`, …): exportar otra vez el mismo clip no debe pisar el
// export anterior.
fn free_path(dir: &std::path::Path, stem: &str) -> std::path::PathBuf {
    (1..1000u32)
        .map(|n| dir.join(if n == 1 { format!("{stem}.mp4") } else { format!("{stem}_{n}.mp4") }))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{stem}.mp4")))
}

#[derive(serde::Deserialize)]
struct ToastPayload {
    title: String,
    body: String,
    #[serde(default)]
    keys: Vec<String>,
    kind: String,
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn toast(toast: tauri::State<'_, toast::Toast>, payload: ToastPayload) {
    toast.show(toast::ToastData {
        title: payload.title,
        body: payload.body,
        keys: payload.keys,
        kind: toast::ToastKind::from_str(&payload.kind),
    });
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn toast(_payload: ToastPayload) {}

#[cfg(target_os = "windows")]
#[tauri::command]
fn dismiss_toast(toast: tauri::State<'_, toast::Toast>) {
    toast.hide();
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn dismiss_toast() {}

// Trae la ventana principal al frente (desde la bandeja o el atajo de abrir).
fn show_main(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    // Instancia única: si se intenta abrir una segunda, se enfoca la existente y la nueva
    // sale. Debe ir como primer plugin para cortar antes de crear ventanas.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }));
    }
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            use tauri::Manager;
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_global_shortcut::Builder::new().build())?;
            // Permitir al protocolo asset leer las carpetas de clips y capturas (pueden estar fuera
            // de app_data), o el editor no podría reproducir/leer los archivos guardados ahí.
            config::allow_asset_scopes(app.handle());
            // Rich Presence de Discord: arranca el gestor con el valor persistido (off por defecto).
            discord::init(app.handle().clone(), config::get_discord_rpc(app.handle()));
            // Temporales de compartir: se purgan en segundo plano para no retrasar el arranque.
            let share_dir = share::dir(app.handle());
            std::thread::spawn(move || share::cleanup(&share_dir));
            // Arranque con el sistema: el instalador escribe la clave Run con `--autostart`.
            // En ese caso la app abre directamente en la bandeja (el replay se arma solo en
            // el webview oculto). En un arranque normal la ventana nace oculta (visible:false
            // para no parpadear) y aquí se muestra.
            let autostart = std::env::args().any(|a| a == "--autostart");
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_min_size(Some(tauri::LogicalSize { width: 1366.0, height: 768.0 }));
                let _ = w.set_size(tauri::LogicalSize { width: 1366.0, height: 768.0 });
                if !autostart {
                    let _ = w.show();
                }
            }
            #[cfg(target_os = "windows")]
            app.manage(toast::Toast::spawn());

            // Watcher de juego en primer plano: mantiene fresco el juego rastreado para que el
            // replay pueda cambiar de objetivo al cambiar de juego (no seguir capturando el que
            // se minimizó).
            detect::spawn_watcher(app.handle().clone());

            // Bandeja del sistema. Doble clic izquierdo abre la app; clic derecho abre el
            // menú con "Abrir Flashback" y "Cerrar". El replay sigue corriendo aunque la
            // ventana esté oculta (vive en hilos de Rust, no en la UI).
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
            let open_i = MenuItem::with_id(app, "open", "Abrir Flashback", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Cerrar", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &quit_i])?;
            let mut tray = TrayIconBuilder::with_id("flashback")
                .tooltip("Flashback")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = event {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Cerrar la ventana (botón X) no termina la app: la oculta a la bandeja. Salir de
            // verdad es solo desde el menú de la bandeja ("Cerrar" → app.exit).
            if let Some(main) = app.get_webview_window("main") {
                let main_c = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_c.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            game_hero,
            game_icon,
            detect_game,
            get_seen_games,
            get_disabled_games,
            set_disabled_games,
            get_encoder,
            set_encoder,
            get_discord_rpc,
            set_discord_rpc,
            get_language,
            set_language,
            list_monitors,
            list_audio_inputs,
            start_capture,
            stop_capture,
            capture_status,
            list_clips,
            clips_dir,
            set_clips_dir,
            pick_folder,
            edit_dest,
            prepare_clip_audio,
            load_clip_edit,
            save_clip_edit,
            clips_with_edits,
            keyframe_times,
            frame_times,
            clip_fps,
            clip_thumbnail,
            capture_frame,
            clip_filmstrip,
            export_clip,
            share_prepare,
            share_cancel,
            start_file_drag,
            get_watermark,
            set_watermark,
            get_watermark_corner,
            set_watermark_corner,
            rename_clip,
            delete_clip,
            delete_clips,
            list_playlists,
            create_playlist,
            update_playlist,
            set_playlist_cover,
            clear_playlist_cover,
            delete_playlist,
            playlist_add_clips,
            playlist_remove_clips,
            playlist_set_clips,
            playlist_restore_clips,
            playlist_mark_seen,
            reorder_playlists,
            start_replay,
            stop_replay,
            save_replay,
            replay_active,
            set_native_hotkeys,
            set_record_prefs,
            set_save_sound,
            toast,
            dismiss_toast
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_repeated_export_gets_the_next_free_name() {
        let dir = std::env::temp_dir().join("flashback_free_path");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(super::free_path(&dir, "a_edit"), dir.join("a_edit.mp4"));
        std::fs::write(dir.join("a_edit.mp4"), b"").unwrap();
        assert_eq!(super::free_path(&dir, "a_edit"), dir.join("a_edit_2.mp4"));
        std::fs::write(dir.join("a_edit_2.mp4"), b"").unwrap();
        assert_eq!(super::free_path(&dir, "a_edit"), dir.join("a_edit_3.mp4"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
