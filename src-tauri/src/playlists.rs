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
    // Posición elegida a mano en la lista de playlists. None es "nunca movida": esas van delante,
    // de la más nueva a la más antigua, que es como se listaban antes de poder reordenarlas. Así
    // un índice antiguo se ve igual que siempre y una playlist recién creada aparece arriba.
    #[serde(default)]
    pub pos: Option<u32>,
    #[serde(default)]
    pub clips: Vec<ClipRef>,
}

// Los índices anteriores guardaban la lista como rutas sueltas. `from` deja leer las dos
// formas y escribir siempre la nueva, así que cada índice se migra en su primera edición
// sin un paso de migración aparte. added_ms 0 es "añadido antes de que existiera el campo",
// no el epoch: ordena al final, que es donde estaban.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawRef {
    Path(String),
    Full {
        path: String,
        #[serde(default)]
        added_ms: u64,
        #[serde(default)]
        seen: bool,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(from = "RawRef")]
pub struct ClipRef {
    pub path: String,
    pub added_ms: u64,
    // Si el clip ya se abrió desde la playlist: apaga la marca de recién añadido.
    pub seen: bool,
}

impl From<RawRef> for ClipRef {
    fn from(raw: RawRef) -> Self {
        match raw {
            RawRef::Path(path) => ClipRef {
                path,
                added_ms: 0,
                seen: false,
            },
            RawRef::Full {
                path,
                added_ms,
                seen,
            } => ClipRef {
                path,
                added_ms,
                seen,
            },
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// Un índice que existe pero no se entiende (escritura cortada, o una versión más nueva que
// cambió el formato) no se trata como vacío: se aparta con otro nombre antes de seguir. Si no,
// la siguiente edición escribiría la lista vacía encima y se perderían todas las playlists.
fn read_all(index: &Path) -> Vec<Playlist> {
    let text = match std::fs::read_to_string(index) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    match serde_json::from_str::<Vec<Playlist>>(&text) {
        Ok(list) => list,
        Err(e) => {
            let aside = index.with_extension(format!("bad-{}.json", now_ms()));
            eprintln!("playlists: índice ilegible ({e}), se aparta a {}", aside.display());
            let _ = std::fs::rename(index, &aside);
            Vec::new()
        }
    }
}

// Temporal + rename: si la app muere a media escritura queda el índice anterior entero, nunca
// uno truncado. En Windows rename reemplaza el destino (MOVEFILE_REPLACE_EXISTING).
fn write_all(index: &Path, list: &[Playlist]) -> Result<(), String> {
    if let Some(parent) = index.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(list).map_err(|e| e.to_string())?;
    let tmp = index.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, index).map_err(|e| e.to_string())
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

pub fn create(index: &Path, name: &str, description: &str) -> Result<Playlist, String> {
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
        description: description.trim().to_string(),
        cover: None,
        cover_ms: 0,
        pos: None,
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

// Recibe la lista entera en su orden nuevo. Una playlist que no venga (creada en otra ventana
// mientras se arrastraba) se queda sin posición y por tanto arriba, en vez de perderse.
pub fn reorder(index: &Path, ids: &[String]) -> Result<(), String> {
    edit(index, |list| {
        for p in list.iter_mut() {
            p.pos = ids.iter().position(|x| x == &p.id).map(|i| i as u32);
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

// Añadir respeta el orden de llegada y no duplica: un clip ya presente se queda donde estaba,
// y conserva su fecha de añadido en vez de volver a marcarse como recién llegado.
pub fn add_clips(index: &Path, id: &str, paths: &[String]) -> Result<(), String> {
    let added_ms = now_ms();
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            for path in paths {
                if !p.clips.iter().any(|c| &c.path == path) {
                    p.clips.push(ClipRef {
                        path: path.clone(),
                        added_ms,
                        seen: false,
                    });
                }
            }
        }
    })
}

pub fn remove_clips(index: &Path, id: &str, paths: &[String]) -> Result<(), String> {
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            p.clips.retain(|c| !paths.iter().any(|x| x == &c.path));
        }
    })
}

// Deshacer un quitar: cada clip vuelve a su índice con su fecha y su marca de visto, no como
// recién añadido al final. Se insertan de menor a mayor índice para que cada posición cuente
// con los que ya han vuelto, y uno que ya esté (se volvió a añadir mientras tanto) no se
// duplica.
pub fn restore_clips(index: &Path, id: &str, mut entries: Vec<(usize, ClipRef)>) -> Result<(), String> {
    entries.sort_by_key(|(i, _)| *i);
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            for (i, clip) in entries {
                if p.clips.iter().any(|c| c.path == clip.path) {
                    continue;
                }
                let at = i.min(p.clips.len());
                p.clips.insert(at, clip);
            }
        }
    })
}

// Reordenar no es volver a añadir: las rutas que ya estaban conservan su fecha y su marca.
pub fn set_clips(index: &Path, id: &str, paths: Vec<String>) -> Result<(), String> {
    let added_ms = now_ms();
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            p.clips = paths
                .into_iter()
                .map(|path| match p.clips.iter().find(|c| c.path == path) {
                    Some(old) => old.clone(),
                    None => ClipRef {
                        path,
                        added_ms,
                        seen: false,
                    },
                })
                .collect();
        }
    })
}

pub fn mark_seen(index: &Path, id: &str, path: &str) -> Result<(), String> {
    edit(index, |list| {
        if let Some(p) = list.iter_mut().find(|p| p.id == id) {
            if let Some(c) = p.clips.iter_mut().find(|c| c.path == path) {
                c.seen = true;
            }
        }
    })
}

// Un clip renombrado cambia de ruta; sin re-mapear desaparecería de todas sus playlists.
pub fn rekey(index: &Path, old: &str, new: &str) {
    let _ = edit(index, |list| {
        for p in list.iter_mut() {
            for c in p.clips.iter_mut() {
                if c.path == old {
                    c.path = new.to_string();
                }
            }
        }
    });
}

pub fn forget(index: &Path, paths: &[String]) {
    let _ = edit(index, |list| {
        for p in list.iter_mut() {
            p.clips.retain(|c| !paths.iter().any(|x| x == &c.path));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Forma exacta del índice que escribían las versiones anteriores: la lista de clips son
    // rutas sueltas, sin fecha. Si el parseo fallara aquí, la app arrancaría con todas las
    // playlists vacías y reescribiría el índice ya vacío.
    const LEGACY: &str = r#"[{"id":"pl1","name":"Clutches","created_ms":1700000000000,
      "clips":["C:\\clips\\a.mp4","C:\\clips\\b.mp4"]}]"#;

    fn parse(json: &str) -> Vec<Playlist> {
        serde_json::from_str(json).expect("el índice se lee")
    }

    #[test]
    fn a_legacy_index_keeps_every_clip() {
        let list = parse(LEGACY);
        let paths: Vec<&str> = list[0].clips.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, [r"C:\clips\a.mp4", r"C:\clips\b.mp4"]);
    }

    // Sin fecha conocida, no una fecha inventada: 0 ordena al final, que es donde estaban.
    #[test]
    fn a_legacy_clip_has_no_added_date_and_is_unseen() {
        let list = parse(LEGACY);
        assert_eq!(list[0].clips[0].added_ms, 0);
        assert!(!list[0].clips[0].seen);
    }

    #[test]
    fn rewriting_a_legacy_index_migrates_it_to_the_new_shape() {
        let json = serde_json::to_string(&parse(LEGACY)).expect("se serializa");
        assert!(json.contains(r#""added_ms":0"#), "{json}");
        let again = parse(&json);
        assert_eq!(again[0].clips.len(), 2);
        assert_eq!(again[0].clips[1].path, r"C:\clips\b.mp4");
    }

    fn temp_index(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("flashback-pl-{name}-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("playlists.json")
    }

    #[test]
    fn an_unreadable_index_is_set_aside_not_overwritten() {
        let index = temp_index("bad");
        std::fs::write(&index, "[{\"id\":").unwrap();
        create(&index, "Nueva", "").unwrap();
        let dir = index.parent().unwrap();
        let aside: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bad-"))
            .collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(std::fs::read_to_string(aside[0].path()).unwrap(), "[{\"id\":");
        assert_eq!(list(&index).len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn set_clips_reorders_and_keeps_dates() {
        let index = temp_index("order");
        let pl = create(&index, "Orden", "").unwrap();
        add_clips(&index, &pl.id, &["a".into(), "b".into(), "c".into()]).unwrap();
        mark_seen(&index, &pl.id, "b").unwrap();
        let before = list(&index)[0].clips.clone();
        set_clips(&index, &pl.id, vec!["c".into(), "a".into(), "b".into()]).unwrap();
        let after = &list(&index)[0].clips;
        let paths: Vec<&str> = after.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["c", "a", "b"]);
        assert!(after[2].seen);
        assert_eq!(after[0].added_ms, before[2].added_ms);
        assert!(!index.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(index.parent().unwrap());
    }

    #[test]
    fn reorder_sets_positions_and_leaves_unknown_ones_unplaced() {
        let index = temp_index("plorder");
        let a = create(&index, "A", "").unwrap();
        let b = create(&index, "B", "").unwrap();
        let c = create(&index, "C", "").unwrap();
        reorder(&index, &[c.id.clone(), a.id.clone()]).unwrap();
        let pos = |id: &str| list(&index).into_iter().find(|p| p.id == id).unwrap().pos;
        assert_eq!(pos(&c.id), Some(0));
        assert_eq!(pos(&a.id), Some(1));
        assert_eq!(pos(&b.id), None);
        let _ = std::fs::remove_dir_all(index.parent().unwrap());
    }

    #[test]
    fn create_stores_the_description_in_the_same_write() {
        let index = temp_index("desc");
        let pl = create(&index, "  Clutches ", "  las buenas  ").unwrap();
        let saved = &list(&index)[0];
        assert_eq!(saved.id, pl.id);
        assert_eq!(saved.name, "Clutches");
        assert_eq!(saved.description, "las buenas");
        let _ = std::fs::remove_dir_all(index.parent().unwrap());
    }

    #[test]
    fn restore_puts_clips_back_where_they_were_with_their_dates() {
        let index = temp_index("restore");
        let pl = create(&index, "R", "").unwrap();
        add_clips(&index, &pl.id, &["a".into(), "b".into(), "c".into(), "d".into()]).unwrap();
        mark_seen(&index, &pl.id, "b").unwrap();
        let before = list(&index)[0].clips.clone();
        remove_clips(&index, &pl.id, &["b".into(), "d".into()]).unwrap();
        let removed = vec![(1, before[1].clone()), (3, before[3].clone())];
        restore_clips(&index, &pl.id, removed).unwrap();
        let after = &list(&index)[0].clips;
        let paths: Vec<&str> = after.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["a", "b", "c", "d"]);
        assert!(after[1].seen);
        assert_eq!(after[3].added_ms, before[3].added_ms);
        let _ = std::fs::remove_dir_all(index.parent().unwrap());
    }

    #[test]
    fn restore_skips_a_clip_that_came_back_meanwhile() {
        let index = temp_index("restoredup");
        let pl = create(&index, "R", "").unwrap();
        add_clips(&index, &pl.id, &["a".into(), "b".into()]).unwrap();
        let before = list(&index)[0].clips.clone();
        remove_clips(&index, &pl.id, &["a".into()]).unwrap();
        add_clips(&index, &pl.id, &["a".into()]).unwrap();
        restore_clips(&index, &pl.id, vec![(0, before[0].clone())]).unwrap();
        assert_eq!(list(&index)[0].clips.len(), 2);
        let _ = std::fs::remove_dir_all(index.parent().unwrap());
    }

    #[test]
    fn a_legacy_playlist_has_no_position() {
        assert_eq!(parse(LEGACY)[0].pos, None);
    }

    // Un índice a medio migrar: una playlist tocada por la versión nueva y otra no.
    #[test]
    fn both_shapes_can_live_in_the_same_index() {
        let list = parse(
            r#"[{"id":"pl1","name":"Vieja","created_ms":1,"clips":["a.mp4"]},
                {"id":"pl2","name":"Nueva","created_ms":2,
                 "clips":[{"path":"b.mp4","added_ms":1700000000000,"seen":true}]}]"#,
        );
        assert_eq!(list[0].clips[0].added_ms, 0);
        assert_eq!(list[1].clips[0].added_ms, 1_700_000_000_000);
        assert!(list[1].clips[0].seen);
    }
}
