use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// Metadatos editados a mano de cada clip (juego y portada), por ruta, en un índice de app-data.
// Nunca se escriben en el vídeo: así sirven igual para MKV/WebM y el original no se toca.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct ClipMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(default)]
    pub cover_ms: u64,
}

pub type Index = BTreeMap<String, ClipMeta>;

pub fn all(index: &Path) -> Index {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    read_all(index)
}

// Juego elegido a mano; None o vacío vuelve a lo detectado al clipear.
pub fn set_game(index: &Path, paths: &[String], game: Option<&str>) -> Result<(), String> {
    let game = game.map(str::trim).filter(|g| !g.is_empty()).map(String::from);
    edit(index, |map| {
        for path in paths {
            map.entry(path.clone()).or_default().game = game.clone();
        }
    })
}

// Cada clip lleva su propio PNG (ya recortado por el frontend), con la hora en el nombre: una
// portada nueva cambia de ruta y el WebView no sirve la anterior desde su caché.
pub fn set_cover(index: &Path, dir: &Path, paths: &[String], bytes: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stamp = now_ms();
    let mut written = Vec::with_capacity(paths.len());
    for path in paths {
        let file = dir.join(format!("{:016x}-{stamp}-{}.png", path_hash(path), COUNTER.fetch_add(1, Ordering::Relaxed)));
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        written.push((path.clone(), file.to_string_lossy().into_owned()));
    }
    let old = edit(index, |map| {
        let mut old = Vec::new();
        for (path, file) in written {
            let meta = map.entry(path).or_default();
            old.extend(meta.cover.replace(file));
            meta.cover_ms = stamp;
        }
        old
    })?;
    for f in old {
        let _ = std::fs::remove_file(f);
    }
    Ok(())
}

pub fn clear_cover(index: &Path, paths: &[String]) -> Result<(), String> {
    let old = edit(index, |map| {
        let mut old = Vec::new();
        for path in paths {
            if let Some(meta) = map.get_mut(path) {
                old.extend(meta.cover.take());
                meta.cover_ms = 0;
            }
        }
        old
    })?;
    for f in old {
        let _ = std::fs::remove_file(f);
    }
    Ok(())
}

// Un clip renombrado cambia de ruta: su juego y su portada se mueven con él.
pub fn rekey(index: &Path, old: &str, new: &str) {
    let _ = edit(index, |map| {
        if let Some(meta) = map.remove(old) {
            map.insert(new.to_string(), meta);
        }
    });
}

// El clip exportado desde el editor se queda con el juego y la portada del original. La portada
// se copia: borrar uno de los dos clips no debe dejar al otro sin imagen.
pub fn inherit(index: &Path, dir: &Path, from: &str, to: &str) -> Result<(), String> {
    let Some(meta) = all(index).remove(from) else { return Ok(()) };
    let to = vec![to.to_string()];
    set_game(index, &to, meta.game.as_deref())?;
    if let Some(bytes) = meta.cover.and_then(|c| std::fs::read(c).ok()) {
        set_cover(index, dir, &to, &bytes)?;
    }
    Ok(())
}

pub fn forget(index: &Path, paths: &[String]) {
    let covers = edit(index, |map| {
        paths.iter().filter_map(|p| map.remove(p)).filter_map(|m| m.cover).collect::<Vec<_>>()
    })
    .unwrap_or_default();
    for f in covers {
        let _ = std::fs::remove_file(f);
    }
}

static LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicU32 = AtomicU32::new(0);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn path_hash(path: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.to_lowercase().hash(&mut h);
    h.finish()
}

// Mismo criterio que las playlists: un índice ilegible se aparta en vez de sobrescribirse, y se
// escribe con temporal + rename para no dejarlo nunca a medias.
fn read_all(index: &Path) -> Index {
    let Ok(text) = std::fs::read_to_string(index) else {
        return Index::new();
    };
    match serde_json::from_str::<Index>(&text) {
        Ok(map) => map,
        Err(e) => {
            let aside = index.with_extension(format!("bad-{}.json", now_ms()));
            eprintln!("clipmeta: índice ilegible ({e}), se aparta a {}", aside.display());
            let _ = std::fs::rename(index, &aside);
            Index::new()
        }
    }
}

fn write_all(index: &Path, map: &Index) -> Result<(), String> {
    if let Some(parent) = index.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(map).map_err(|e| e.to_string())?;
    let tmp = index.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, index).map_err(|e| e.to_string())
}

fn edit<T>(index: &Path, f: impl FnOnce(&mut Index) -> T) -> Result<T, String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut map = read_all(index);
    let out = f(&mut map);
    map.retain(|_, m| m.game.is_some() || m.cover.is_some());
    write_all(index, &map)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(name: &str) -> (Tmp, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("fb_clipmeta_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let index = dir.join("clipmeta.json");
        let covers = dir.join("covers");
        (Tmp(dir), index, covers)
    }

    fn p(s: &str) -> Vec<String> {
        vec![s.to_string()]
    }

    #[test]
    fn a_game_is_stored_per_clip_and_can_be_cleared() {
        let (_t, index, _) = tmp("game");
        set_game(&index, &["a.mp4".into(), "b.mp4".into()], Some("  VALORANT ")).unwrap();
        let m = all(&index);
        assert_eq!(m["a.mp4"].game.as_deref(), Some("VALORANT"));
        assert_eq!(m["b.mp4"].game.as_deref(), Some("VALORANT"));
        set_game(&index, &p("a.mp4"), None).unwrap();
        set_game(&index, &p("b.mp4"), Some("   ")).unwrap();
        assert!(all(&index).is_empty(), "sin juego ni portada no queda entrada");
    }

    #[test]
    fn a_cover_is_written_replaced_and_cleared() {
        let (_t, index, covers) = tmp("cover");
        set_cover(&index, &covers, &p("a.mp4"), b"png-1").unwrap();
        let first = all(&index)["a.mp4"].cover.clone().unwrap();
        assert_eq!(std::fs::read(&first).unwrap(), b"png-1");
        std::thread::sleep(std::time::Duration::from_millis(2));
        set_cover(&index, &covers, &p("a.mp4"), b"png-2").unwrap();
        let second = all(&index)["a.mp4"].cover.clone().unwrap();
        assert_ne!(first, second, "otra ruta para que el WebView no sirva la caché");
        assert!(!Path::new(&first).exists(), "la portada anterior se borra");
        assert!(all(&index)["a.mp4"].cover_ms > 0);
        clear_cover(&index, &p("a.mp4")).unwrap();
        assert!(!Path::new(&second).exists());
        assert!(all(&index).is_empty());
    }

    #[test]
    fn several_clips_get_their_own_cover_file() {
        let (_t, index, covers) = tmp("bulk");
        set_cover(&index, &covers, &["a.mp4".into(), "b.mp4".into()], b"png").unwrap();
        let m = all(&index);
        assert_ne!(m["a.mp4"].cover, m["b.mp4"].cover);
        clear_cover(&index, &p("a.mp4")).unwrap();
        assert!(Path::new(m["b.mp4"].cover.as_ref().unwrap()).exists());
    }

    #[test]
    fn renaming_keeps_the_metadata_and_deleting_drops_it() {
        let (_t, index, covers) = tmp("rekey");
        set_game(&index, &p("a.mp4"), Some("Overwatch")).unwrap();
        set_cover(&index, &covers, &p("a.mp4"), b"png").unwrap();
        rekey(&index, "a.mp4", "b.mp4");
        let m = all(&index);
        assert!(!m.contains_key("a.mp4"));
        assert_eq!(m["b.mp4"].game.as_deref(), Some("Overwatch"));
        let cover = m["b.mp4"].cover.clone().unwrap();
        forget(&index, &p("b.mp4"));
        assert!(all(&index).is_empty());
        assert!(!Path::new(&cover).exists());
    }

    #[test]
    fn an_export_inherits_game_and_its_own_copy_of_the_cover() {
        let (_t, index, covers) = tmp("inherit");
        set_game(&index, &p("a.mp4"), Some("VALORANT")).unwrap();
        set_cover(&index, &covers, &p("a.mp4"), b"png").unwrap();
        inherit(&index, &covers, "a.mp4", "a_edit.mp4").unwrap();
        let m = all(&index);
        assert_eq!(m["a_edit.mp4"].game.as_deref(), Some("VALORANT"));
        let (orig, copy) = (m["a.mp4"].cover.clone().unwrap(), m["a_edit.mp4"].cover.clone().unwrap());
        assert_ne!(orig, copy);
        assert_eq!(std::fs::read(&copy).unwrap(), b"png");
        inherit(&index, &covers, "sin-meta.mp4", "otro.mp4").unwrap();
        assert!(!all(&index).contains_key("otro.mp4"));
    }

    #[test]
    fn an_unreadable_index_is_set_aside() {
        let (t, index, _) = tmp("bad");
        std::fs::write(&index, b"{no es json").unwrap();
        assert!(all(&index).is_empty());
        set_game(&index, &p("a.mp4"), Some("VALORANT")).unwrap();
        let aside = std::fs::read_dir(&t.0)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("clipmeta.bad-"));
        assert!(aside, "el índice roto se conserva aparte");
        assert_eq!(all(&index)["a.mp4"].game.as_deref(), Some("VALORANT"));
    }
}
