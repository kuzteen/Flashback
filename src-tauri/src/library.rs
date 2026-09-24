use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(serde::Serialize)]
pub struct ClipInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_ms: i64,
    pub duration_sec: f64,
    pub source: String,
}

pub fn list_clips(dirs: Vec<PathBuf>) -> Vec<ClipInfo> {
    let mut out = Vec::new();
    for dir in dirs {
        scan_dir(&dir, &mut out);
    }
    // Dedup por ruta por si dos carpetas escaneadas se solapan; se conserva la primera.
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.path.to_lowercase()));
    out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    out
}

fn scan_dir(dir: &Path, out: &mut Vec<ClipInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_mp4 = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
        if !is_mp4 {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let id = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Clip")
            .to_string();
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let source = clip_source(&path).unwrap_or_default();
        out.push(ClipInfo {
            id,
            name,
            path: path.to_string_lossy().into_owned(),
            size_bytes: meta.len(),
            modified_ms,
            duration_sec: mp4_duration_secs(&path).unwrap_or(0.0),
            source,
        });
    }
}

// Sidecars que acompañan a cada MP4 (metadato de fuente y edición no destructiva). Renombrar
// o borrar un clip debe arrastrarlos para no dejar huérfanos.
const SIDECARS: [&str; 2] = ["clip.json", "edit.json"];

// Renombra el clip (y sus sidecars). Valida el nombre y evita pisar otro clip. Devuelve la
// nueva ruta del MP4.
pub fn rename_clip(path: &str, new_name: &str, edit_index: &Path) -> Result<String, String> {
    let p = Path::new(path);
    let parent = p.parent().ok_or("Ruta inválida")?;
    let name = new_name.trim();
    if name.is_empty() {
        return Err("El nombre no puede estar vacío".into());
    }
    if name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("El nombre contiene caracteres no válidos".into());
    }
    let new_mp4 = parent.join(format!("{name}.mp4"));
    if new_mp4 == p {
        return Ok(path.to_string());
    }
    if new_mp4.exists() {
        return Err("Ya existe un clip con ese nombre".into());
    }
    std::fs::rename(p, &new_mp4).map_err(|e| e.to_string())?;
    for ext in SIDECARS {
        let from = p.with_extension(ext);
        if from.exists() {
            let _ = std::fs::rename(&from, new_mp4.with_extension(ext));
        }
    }
    let new_path = new_mp4.to_string_lossy().into_owned();
    // La edición vive en el índice de app-data, indexada por ruta: re-mapear la entrada al
    // nuevo nombre para no perder el montaje al renombrar.
    crate::edits::rekey(edit_index, path, &new_path);
    Ok(new_path)
}

// Envía el clip y sus sidecars a la papelera (recuperable). El borrado es la única operación
// destructiva de la app, así que se usa la papelera del sistema en vez de un borrado directo.

// El lote va en una sola llamada a la papelera: una única operación del shell y una sola
// entrada de deshacer para el usuario, en vez de una por clip.
pub fn delete_clips(paths: &[String], edit_index: &Path) -> Result<(), String> {
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let p = Path::new(path);
        files.push(p.to_path_buf());
        for ext in SIDECARS {
            let s = p.with_extension(ext);
            if s.exists() {
                files.push(s);
            }
        }
    }
    recycle(&files)?;
    crate::edits::remove_many(edit_index, paths);
    Ok(())
}

#[cfg(windows)]
fn recycle(paths: &[PathBuf]) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE,
        SHFILEOPSTRUCTW,
    };
    // pFrom es una lista de rutas separadas por NUL y terminada en doble NUL.
    let mut from: Vec<u16> = Vec::new();
    for p in paths {
        from.extend(p.as_os_str().encode_wide());
        from.push(0);
    }
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE as u32,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI).0 as u16,
        ..Default::default()
    };
    let rc = unsafe { SHFileOperationW(&mut op) };
    if rc != 0 {
        return Err(format!("No se pudo enviar a la papelera (código {rc})"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn recycle(paths: &[PathBuf]) -> Result<(), String> {
    for p in paths {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn read_u32(f: &mut File) -> Option<u32> {
    let mut b = [0u8; 4];
    f.read_exact(&mut b).ok()?;
    Some(u32::from_be_bytes(b))
}

fn read_u64(f: &mut File) -> Option<u64> {
    let mut b = [0u8; 8];
    f.read_exact(&mut b).ok()?;
    Some(u64::from_be_bytes(b))
}

// Duración leyendo el árbol de cajas ISO-BMFF: se recorren las cajas de nivel
// superior saltando `mdat` por su tamaño (sin leer su contenido) hasta `moov`, y
// dentro de `moov` se busca `mvhd` (timescale + duration). Sin dependencias.
pub fn mp4_duration_secs(path: &Path) -> Option<f64> {
    let mut f = File::open(path).ok()?;
    let file_len = f.metadata().ok()?.len();
    let mut pos = 0u64;
    while let Some((typ, body, end)) = next_box(&mut f, pos, file_len) {
        if &typ == b"moov" {
            let mvhd = find_child(&mut f, body, end, b"mvhd")?;
            let (timescale, duration) = header_times(&mut f, mvhd)?;
            // Grabación que no llegó a cerrarse (cierre inesperado): MP4 fragmentado cuyo `moov`
            // no sabe la duración; se suma la de los fragmentos.
            if duration == 0 && find_child(&mut f, body, end, b"mvex").is_some() {
                return fragmented_duration(&mut f, body, end, file_len);
            }
            return (timescale > 0).then(|| duration as f64 / timescale as f64);
        }
        pos = end;
    }
    None
}

// Siguiente caja en `pos`: (tipo, inicio del cuerpo, fin de la caja).
fn next_box(f: &mut File, pos: u64, end: u64) -> Option<([u8; 4], u64, u64)> {
    if pos + 8 > end {
        return None;
    }
    f.seek(SeekFrom::Start(pos)).ok()?;
    let size32 = read_u32(f)?;
    let mut typ = [0u8; 4];
    f.read_exact(&mut typ).ok()?;
    let (box_size, header) = box_extent(f, size32, pos, end)?;
    (box_size >= header).then_some((typ, pos + header, pos + box_size))
}

fn find_child(f: &mut File, start: u64, end: u64, want: &[u8; 4]) -> Option<(u64, u64)> {
    let mut pos = start;
    while let Some((typ, body, box_end)) = next_box(f, pos, end) {
        if &typ == want {
            return Some((body, box_end));
        }
        pos = box_end;
    }
    None
}

// (timescale, duration) de un `mvhd` o `mdhd`, que comparten disposición.
fn header_times(f: &mut File, (body, _): (u64, u64)) -> Option<(u64, u64)> {
    f.seek(SeekFrom::Start(body)).ok()?;
    let mut version_flags = [0u8; 4];
    f.read_exact(&mut version_flags).ok()?;
    if version_flags[0] == 1 {
        f.seek(SeekFrom::Current(16)).ok()?; // creation(8) + modification(8)
        Some((read_u32(f)? as u64, read_u64(f)?))
    } else {
        f.seek(SeekFrom::Current(8)).ok()?; // creation(4) + modification(4)
        Some((read_u32(f)? as u64, read_u32(f)? as u64))
    }
}

// Fin de la primera pista (el vídeo) según sus fragmentos: `tfdt` + duraciones del `trun`.
fn fragmented_duration(f: &mut File, moov: u64, moov_end: u64, file_len: u64) -> Option<f64> {
    let (trak, trak_end) = find_child(f, moov, moov_end, b"trak")?;
    let (tkhd, _) = find_child(f, trak, trak_end, b"tkhd")?;
    f.seek(SeekFrom::Start(tkhd)).ok()?;
    let v1 = read_u32(f)? >> 24 == 1;
    f.seek(SeekFrom::Current(if v1 { 16 } else { 8 })).ok()?;
    let track_id = read_u32(f)?;
    let (mdia, mdia_end) = find_child(f, trak, trak_end, b"mdia")?;
    let mdhd = find_child(f, mdia, mdia_end, b"mdhd")?;
    let (timescale, _) = header_times(f, mdhd)?;
    if timescale == 0 {
        return None;
    }

    let mut end_ticks = 0u64;
    let mut pos = moov_end;
    while let Some((typ, body, box_end)) = next_box(f, pos, file_len) {
        if &typ == b"moof" {
            let mut t = body;
            while let Some((ttyp, tbody, tend)) = next_box(f, t, box_end) {
                if &ttyp == b"traf" {
                    if let Some(end) = traf_end(f, tbody, tend, track_id) {
                        end_ticks = end_ticks.max(end);
                    }
                }
                t = tend;
            }
        }
        pos = box_end;
    }
    Some(end_ticks as f64 / timescale as f64)
}

fn traf_end(f: &mut File, start: u64, end: u64, track_id: u32) -> Option<u64> {
    let read_body = |f: &mut File, (body, box_end): (u64, u64)| -> Option<Vec<u8>> {
        f.seek(SeekFrom::Start(body)).ok()?;
        let mut v = vec![0u8; (box_end - body) as usize];
        f.read_exact(&mut v).ok()?;
        Some(v)
    };
    let be32 = |d: &[u8], at: usize| -> Option<u32> { Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?)) };

    let at = find_child(f, start, end, b"tfhd")?;
    let tfhd = read_body(f, at)?;
    if be32(&tfhd, 4)? != track_id {
        return None;
    }
    let tfhd_flags = be32(&tfhd, 0)? & 0xFF_FFFF;
    let dur_at = 8 + if tfhd_flags & 0x1 != 0 { 8 } else { 0 } + if tfhd_flags & 0x2 != 0 { 4 } else { 0 };
    let default_dur = if tfhd_flags & 0x8 != 0 { be32(&tfhd, dur_at)? } else { 0 };

    let at = find_child(f, start, end, b"tfdt")?;
    let tfdt = read_body(f, at)?;
    let base = if tfdt[0] == 1 {
        u64::from_be_bytes(tfdt.get(4..12)?.try_into().ok()?)
    } else {
        be32(&tfdt, 4)? as u64
    };

    let at = find_child(f, start, end, b"trun")?;
    let trun = read_body(f, at)?;
    let flags = be32(&trun, 0)? & 0xFF_FFFF;
    let count = be32(&trun, 4)? as usize;
    let mut at = 8 + if flags & 0x1 != 0 { 4 } else { 0 } + if flags & 0x4 != 0 { 4 } else { 0 };
    let per_sample = [0x100, 0x200, 0x400, 0x800].iter().filter(|b| flags & **b != 0).count() * 4;
    let mut total = 0u64;
    for _ in 0..count {
        total += if flags & 0x100 != 0 { be32(&trun, at)? } else { default_dur } as u64;
        at += per_sample;
    }
    Some(base + total)
}

// Resuelve el tamaño real de una caja y su cabecera: tamaño 1 = largesize de 64
// bits que sigue al tipo; tamaño 0 = la caja se extiende hasta el final.
fn box_extent(f: &mut File, size32: u32, pos: u64, container_end: u64) -> Option<(u64, u64)> {
    match size32 {
        1 => Some((read_u64(f)?, 16)),
        0 => Some((container_end - pos, 8)),
        n => Some((n as u64, 8)),
    }
}

// Metadatos propios del clip (de momento, solo el origen: juego o monitor) embebidos EN el MP4
// para que viajen con el archivo sin sidecars. Se guardan en una caja `uuid` de nivel superior
// (punto de extensión sancionado por ISO-BMFF): los reproductores ignoran las `uuid` que no
// reconocen y, al ir tras `mdat`, no se altera el vídeo ni las tablas de offsets.
const FB_META_UUID: [u8; 16] = [
    0xfb, 0x1a, 0x5b, 0xac, 0x46, 0x4c, 0x41, 0x53, 0x48, 0x42, 0x41, 0x43, 0x4b, 0x4d, 0x44, 0x31,
];

// Origen del clip (juego/monitor): primero el metadato embebido en el MP4; si no está (clip
// antiguo), el sidecar `.clip.json` heredado.
pub fn clip_source(path: &Path) -> Option<String> {
    read_embedded_source(path).or_else(|| legacy_source(path))
}

pub fn read_embedded_source(path: &Path) -> Option<String> {
    let json = read_meta_box(path)?;
    let v: serde_json::Value = serde_json::from_slice(&json).ok()?;
    v.get("source")?.as_str().map(String::from)
}

fn read_meta_box(path: &Path) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    let file_len = f.metadata().ok()?.len();
    let mut pos = 0u64;
    while pos + 8 <= file_len {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let size32 = read_u32(&mut f)?;
        let mut typ = [0u8; 4];
        f.read_exact(&mut typ).ok()?;
        let (box_size, header) = box_extent(&mut f, size32, pos, file_len)?;
        if &typ == b"uuid" && box_size >= header + 16 {
            f.seek(SeekFrom::Start(pos + header)).ok()?;
            let mut sig = [0u8; 16];
            if f.read_exact(&mut sig).is_ok() && sig == FB_META_UUID {
                let payload_len = (box_size - header - 16) as usize;
                let mut buf = vec![0u8; payload_len];
                f.read_exact(&mut buf).ok()?;
                return Some(buf);
            }
        }
        if box_size < header {
            break;
        }
        pos += box_size;
    }
    None
}

pub fn write_embedded_source(path: &Path, source: &str) -> std::io::Result<()> {
    let payload = serde_json::json!({ "source": source }).to_string();
    let payload = payload.as_bytes();
    let size = 8 + 16 + payload.len();
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(&(size as u32).to_be_bytes());
    bytes.extend_from_slice(b"uuid");
    bytes.extend_from_slice(&FB_META_UUID);
    bytes.extend_from_slice(payload);
    let mut f = std::fs::OpenOptions::new().append(true).open(path)?;
    f.write_all(&bytes)
}

// Compatibilidad hacia atrás: clips creados antes del metadato embebido guardan el origen en un
// sidecar `<clip>.clip.json`. Se sigue leyendo si el MP4 no lleva la caja propia.
fn legacy_source(path: &Path) -> Option<String> {
    let sidecar = path.with_extension("clip.json");
    std::fs::read_to_string(&sidecar)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("source")?.as_str().map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_clips_aggregates_and_dedupes() {
        let base = std::env::temp_dir().join(format!("fb_list_test_{}", std::process::id()));
        let a = base.join("a");
        let b = base.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("one.mp4"), b"\x00\x00\x00\x08ftypisom").unwrap();
        std::fs::write(b.join("two.mp4"), b"\x00\x00\x00\x08ftypisom").unwrap();
        std::fs::write(b.join("note.txt"), b"x").unwrap();

        // 'a' repetida: el dedup por ruta evita entradas duplicadas; el .txt se ignora.
        let clips = list_clips(vec![a.clone(), b.clone(), a.clone()]);
        let ids: Vec<_> = clips.iter().map(|c| c.id.clone()).collect();
        assert_eq!(clips.len(), 2, "ids={ids:?}");
        assert!(ids.contains(&"one.mp4".to_string()));
        assert!(ids.contains(&"two.mp4".to_string()));

        std::fs::remove_dir_all(&base).ok();
    }

    fn recording(name: &str, finish: bool) -> PathBuf {
        use crate::mp4mux::{hybrid::Hybrid, Track};
        let seq = [0, 0, 0, 1, 0x67, 0x4D, 0x00, 0x1F, 0x95, 0xA8, 0, 0, 0, 1, 0x68, 0xEE];
        let p = std::env::temp_dir().join(format!("fb_dur_{name}_{}.mp4", std::process::id()));
        let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&p).unwrap();
        let mut h = Hybrid::new(std::io::BufWriter::new(f), vec![Track::h264(64, 64, &seq).unwrap()]).unwrap();
        for i in 0..30i64 {
            let key = i % 10 == 0;
            let data = vec![0, 0, 0, 1, if key { 0x65 } else { 0x41 }, 0xAA];
            h.push(0, std::sync::Arc::new(data), i * 333_333, 333_333, key).unwrap();
        }
        if finish {
            h.finish().unwrap();
        }
        p
    }

    #[test]
    fn duration_of_an_unfinished_recording_comes_from_its_fragments() {
        let p = recording("open", false);
        let d = mp4_duration_secs(&p);
        std::fs::remove_file(&p).ok();
        let d = d.unwrap();
        assert!((d - 20.0 / 30.0).abs() < 0.01, "duración {d}");
    }

    #[test]
    fn duration_of_a_finished_recording_comes_from_its_index() {
        let p = recording("done", true);
        let d = mp4_duration_secs(&p);
        std::fs::remove_file(&p).ok();
        let d = d.unwrap();
        assert!((d - 1.0).abs() < 0.01, "duración {d}");
    }

    #[test]
    fn embedded_source_roundtrip() {
        let dir = std::env::temp_dir().join(format!("fb_meta_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("clip.mp4");
        // MP4 mínimo con cajas de nivel superior bien dimensionadas (ftyp + mdat falso): basta para
        // que el iterador llegue hasta nuestra caja añadida al final.
        let mut base = Vec::new();
        base.extend_from_slice(&12u32.to_be_bytes());
        base.extend_from_slice(b"ftyp");
        base.extend_from_slice(b"isom");
        base.extend_from_slice(&16u32.to_be_bytes());
        base.extend_from_slice(b"mdat");
        base.extend_from_slice(&[0u8; 8]);
        std::fs::write(&p, &base).unwrap();

        assert_eq!(read_embedded_source(&p), None);
        write_embedded_source(&p, "VALORANT").unwrap();
        assert_eq!(read_embedded_source(&p).as_deref(), Some("VALORANT"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
