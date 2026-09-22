use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// Las playlists son metadato puro: un único archivo en app-data con la lista ordenada de
// rutas de clip. No tocan los ficheros de vídeo ni la carpeta de la biblioteca, así que
// añadir o quitar un clip de una playlist nunca mueve ni copia nada en disco.
static LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub created_ms: u64,
    // Con default: los índices escritos antes de que existieran portada y descripción se
    // siguen leyendo sin migración.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub cover: Option<String>,
    // Marca de la última portada escrita. El archivo conserva el nombre al reemplazarlo, así
    // que sin esto el WebView seguiría mostrando la anterior desde su caché.
    #[serde(default)]
    pub cover_ms: u64,
    #[serde(default)]
    pub clips: Vec<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn read_all(index: &Path) -> Vec<Playlist> {
    std::fs::read_to_string(index)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<Playlist>>(&s).ok())
        .unwrap_or_default()
}

fn write_all(index: &Path, list: &[Playlist]) -> Result<(), String> {
    if let Some(parent) = index.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(list).map_err(|e| e.to_string())?;
    std::fs::write(index, json).map_err(|e| e.to_string())
}

fn edit<T>(index: &Path, f: impl FnOnce(&mut Vec<Playlist>) -> T) -> Result<T, String> {
    let _g = LOCK.lock().unwrap();
    let mut list = read_all(index);
    let out = f(&mut list);
    write_all(index, &list)?;
    Ok(out)
}

pub fn list(index: &Path) -> Vec<Playlist> {
    let _g = LOCK.lock().unwrap();
    read_all(index)
}

pub fn create(index: &Path, name: &str) -> Result<Playlist, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("El nombre no puede estar vacío".into());
    }
    let created_ms = now_ms();
    let pl = Playlist {
        // El contador desempata los ids cuando se crean dos playlists dentro del mismo
        // milisegundo, que es justo lo que pasa al duplicar con el teclado.
        id: format!("pl{created_ms}{:x}", COUNTER.fetch_add(1, Ordering::Relaxed)),
        name: name.to_string(),
        created_ms,
        description: String::new(),
        cover: None,
        cover_ms: 0,
        clips: Vec::new(),
    };
    let out = pl.clone();
    edit(index, |list| list.push(pl))?;
    Ok(out)
}

pub fn update(index: &Path, id: &str, name: &str, description: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("El nombre no puede estar vacío".into());
    }
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            p.name = name.to_string();
            p.description = description.trim().to_string();
        }
    })
}

pub fn remove(index: &Path, id: &str) -> Result<(), String> {
    let cover = edit(index, |list| {
        let cover = list.iter().find(|p| p.id == id).and_then(|p| p.cover.clone());
        list.retain(|p| p.id != id);
        cover
    })?;
    if let Some(c) = cover {
        let _ = std::fs::remove_file(c);
    }
    Ok(())
}

// La portada se guarda siempre como <id>.png en su propia carpeta: el frontend la manda ya
// recortada a cuadrado y reescalada, así que aquí solo hay que escribir bytes.
pub fn set_cover(index: &Path, dir: &Path, id: &str, bytes: &[u8]) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("{id}.png"));
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    let path = file.to_string_lossy().into_owned();
    let stamp = now_ms();
    let found = edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            p.cover = Some(path.clone());
            p.cover_ms = stamp;
            return true;
        }
        false
    })?;
    if !found {
        let _ = std::fs::remove_file(&file);
        return Err("La playlist ya no existe".into());
    }
    Ok(path)
}

pub fn clear_cover(index: &Path, id: &str) -> Result<(), String> {
    let cover = edit(index, |list| {
        list.iter_mut().find(|p| p.id == id).and_then(|p| {
            p.cover_ms = 0;
            p.cover.take()
        })
    })?;
    if let Some(c) = cover {
        let _ = std::fs::remove_file(c);
    }
    Ok(())
}

// Añadir respeta el orden de llegada y no duplica: un clip ya presente se queda donde estaba.
pub fn add_clips(index: &Path, id: &str, paths: &[String]) -> Result<(), String> {
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            for path in paths {
                if !p.clips.iter().any(|c| c == path) {
                    p.clips.push(path.clone());
                }
            }
        }
    })
}

pub fn remove_clips(index: &Path, id: &str, paths: &[String]) -> Result<(), String> {
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            p.clips.retain(|c| !paths.iter().any(|x| x == c));
        }
    })
}

pub fn set_clips(index: &Path, id: &str, paths: Vec<String>) -> Result<(), String> {
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            p.clips = paths;
        }
    })
}

// Un clip renombrado cambia de ruta; sin re-mapear desaparecería de todas sus playlists.
pub fn rekey(index: &Path, old: &str, new: &str) {
    let _ = edit(index, |list| {
        for p in list.iter_mut() {
            for c in p.clips.iter_mut() {
                if c == old {
                    *c = new.to_string();
                }
            }
        }
    });
}

pub fn forget(index: &Path, paths: &[String]) {
    let _ = edit(index, |list| {
        for p in list.iter_mut() {
            p.clips.retain(|c| !paths.iter().any(|x| x == c));
        }
    });
}
