// Tablas de muestras de un MP4 cerrado leídas de su `moov` (unos KB), sin recorrer el archivo. El
// SourceReader de Media Foundation lee el MP4 entero para entregar las muestras de cualquier pista
// (el vídeo va intercalado), y en un clip de 887 MB eso es ~1 s por pasada: listar fotogramas o
// sacar el audio para el editor costaba lo que leer todo el vídeo. Solo cubre lo que escribe
// `mp4mux` —sin B-frames (`ctts`), edit list de tramos vacíos más uno desde 0, AAC-LC crudo—; ante
// cualquier otra cosa devuelve None y el llamador usa Media Foundation.
//
// Los instantes van en unidades de 100 ns y se calculan igual que Media Foundation, para que ambos
// caminos den exactamente los mismos valores.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub struct VideoIndex {
    pub frames: Vec<i64>,
    pub keyframes: Vec<i64>,
}

pub struct AudioTrack {
    pub sample_rate: u32,
    pub channels: u16,
    // MF_MT_USER_DATA del decodificador AAC: la cola de HEAACWAVEINFO (payload crudo) + el
    // AudioSpecificConfig del `esds`.
    pub user_data: Vec<u8>,
    // Tramos contiguos en disco: (offset, tamaño de cada muestra, instante de cada muestra).
    pub chunks: Vec<(u64, Vec<u32>, Vec<i64>)>,
}

pub struct Mp4 {
    moov: Vec<u8>,
}

impl Mp4 {
    pub fn open(path: &Path) -> Option<Mp4> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        if matches!(ext.as_str(), "mkv" | "webm") {
            return None;
        }
        let mut f = File::open(path).ok()?;
        let len = f.metadata().ok()?.len();
        let mut pos = 0u64;
        while pos + 8 <= len {
            f.seek(SeekFrom::Start(pos)).ok()?;
            let mut head = [0u8; 16];
            f.read_exact(&mut head[..8]).ok()?;
            let (size, header) = match be32(&head, 0)? {
                1 => {
                    f.read_exact(&mut head[8..]).ok()?;
                    (be64(&head, 8)?, 16)
                }
                0 => (len - pos, 8),
                n => (n as u64, 8),
            };
            if size < header || pos + size > len {
                return None;
            }
            if &head[4..8] == b"moov" {
                let body = size - header;
                if body > 256 << 20 {
                    return None;
                }
                let mut moov = vec![0u8; body as usize];
                f.read_exact(&mut moov).ok()?;
                if child(&moov, b"mvex").is_some() {
                    return None;
                }
                return Some(Mp4 { moov });
            }
            pos += size;
        }
        None
    }

    fn traks(&self) -> impl Iterator<Item = &[u8]> {
        children(&self.moov).filter(|(t, _)| t == b"trak").map(|(_, b)| b)
    }

    pub fn video(&self) -> Option<VideoIndex> {
        let movie_scale = timescale(child(&self.moov, b"mvhd")?)?;
        let trak = self.traks().find(|t| handler(t) == Some(*b"vide"))?;
        let stbl = find(trak, &[b"mdia", b"minf", b"stbl"])?;
        let frames = sample_times(trak, stbl, movie_scale)?;
        let keyframes = match child(stbl, b"stss") {
            Some(stss) => {
                let mut keys = Vec::new();
                for i in 0..be32(stss, 4)? as usize {
                    keys.push(*frames.get((be32(stss, 8 + i * 4)? as usize).checked_sub(1)?)?);
                }
                keys
            }
            None => frames.clone(),
        };
        (!frames.is_empty()).then_some(VideoIndex { frames, keyframes })
    }

    // Pistas de audio con el mismo ordinal que les da Media Foundation, del que depende qué pista es
    // el sistema y cuál el micro: las numera al revés que el archivo y se salta las vacías (la de
    // sistema de una grabación en la que no sonó nada). Una pista que no se sepa leer queda como
    // None, sin mover el ordinal de las demás.
    pub fn audio(&self) -> Option<Vec<Option<AudioTrack>>> {
        let movie_scale = timescale(child(&self.moov, b"mvhd")?)?;
        let mut tracks: Vec<_> = self
            .traks()
            .filter(|t| handler(t) == Some(*b"soun"))
            .filter(|t| find(t, &[b"mdia", b"minf", b"stbl", b"stsz"]).and_then(|s| be32(s, 8)) != Some(0))
            .map(|t| audio_track(t, movie_scale))
            .collect();
        tracks.reverse();
        Some(tracks)
    }
}

fn audio_track(trak: &[u8], movie_scale: u64) -> Option<AudioTrack> {
    let stbl = find(trak, &[b"mdia", b"minf", b"stbl"])?;
    let stsd = child(stbl, b"stsd")?;
    if be32(stsd, 4)? != 1 {
        return None;
    }
    let (typ, entry) = children(stsd.get(8..)?).next()?;
    // Entrada de sonido ISO (versión 0): canales en 16, frecuencia 16.16 en 24 y las cajas hijas
    // desde 28. Las de QuickTime v1/v2 llevan más campos y se dejan a Media Foundation.
    if &typ != b"mp4a" || be16(entry, 8)? != 0 {
        return None;
    }
    let channels = be16(entry, 16)?;
    let sample_rate = be32(entry, 24)? >> 16;
    let asc = aac_config(child(entry.get(28..)?, b"esds")?)?;
    // Solo AAC-LC: con HE-AAC la frecuencia de la entrada es la del núcleo y no la de salida.
    if asc.first()? >> 3 != 2 || channels == 0 || sample_rate == 0 {
        return None;
    }
    let mut user_data = vec![0u8; 12];
    user_data.extend_from_slice(&asc);

    let times = sample_times(trak, stbl, movie_scale)?;
    let sizes = sample_sizes(child(stbl, b"stsz")?)?;
    if sizes.len() != times.len() {
        return None;
    }
    let offsets = chunk_offsets(stbl)?;
    let stsc = child(stbl, b"stsc")?;
    let entries = be32(stsc, 4)? as usize;
    let mut chunks = Vec::with_capacity(offsets.len());
    let mut next = 0usize;
    for e in 0..entries {
        let first = be32(stsc, 8 + e * 12)? as usize;
        let per_chunk = be32(stsc, 12 + e * 12)? as usize;
        let last = if e + 1 < entries { be32(stsc, 20 + e * 12)? as usize } else { offsets.len() + 1 };
        for c in first..last {
            let offset = *offsets.get(c.checked_sub(1)?)?;
            let end = next.checked_add(per_chunk)?;
            chunks.push((offset, sizes.get(next..end)?.to_vec(), times[next..end].to_vec()));
            next = end;
        }
    }
    (next == times.len()).then_some(AudioTrack { sample_rate, channels, user_data, chunks })
}

// AudioSpecificConfig del `esds`: ES_Descriptor (0x03) → DecoderConfigDescriptor (0x04) →
// DecoderSpecificInfo (0x05).
fn aac_config(esds: &[u8]) -> Option<Vec<u8>> {
    let mut d = esds.get(4..)?;
    let (tag, body) = descriptor(d)?;
    if tag != 0x03 {
        return None;
    }
    let flags = *body.get(2)?;
    let mut at = 3;
    if flags & 0x80 != 0 {
        at += 2;
    }
    if flags & 0x40 != 0 {
        at += 1 + *body.get(at)? as usize;
    }
    if flags & 0x20 != 0 {
        at += 2;
    }
    d = body.get(at..)?;
    let (tag, body) = descriptor(d)?;
    if tag != 0x04 || *body.first()? != 0x40 {
        return None;
    }
    let (tag, asc) = descriptor(body.get(13..)?)?;
    (tag == 0x05 && !asc.is_empty()).then(|| asc.to_vec())
}

fn descriptor(d: &[u8]) -> Option<(u8, &[u8])> {
    let tag = *d.first()?;
    let mut len = 0usize;
    let mut at = 1;
    loop {
        let b = *d.get(at)?;
        at += 1;
        len = (len << 7) | (b & 0x7F) as usize;
        if b & 0x80 == 0 || at > 4 {
            break;
        }
    }
    Some((tag, d.get(at..at.checked_add(len)?)?))
}

// Instante de presentación de cada muestra: tiempos de decodificación (`stts`) desplazados por los
// tramos vacíos iniciales de la edit list, que es como los coloca Media Foundation.
fn sample_times(trak: &[u8], stbl: &[u8], movie_scale: u64) -> Option<Vec<i64>> {
    let scale = timescale(find(trak, &[b"mdia", b"mdhd"])?)?;
    if scale == 0 || movie_scale == 0 || child(stbl, b"ctts").is_some() {
        return None;
    }
    let mut offset = 0i64;
    if let Some(elst) = find(trak, &[b"edts", b"elst"]) {
        let v1 = *elst.first()? == 1;
        let entry = if v1 { 20 } else { 12 };
        let mut media = false;
        for i in 0..be32(elst, 4)? as usize {
            let at = 8 + i * entry;
            let (dur, media_time) = if v1 {
                (be64(elst, at)?, be64(elst, at + 8)? as i64)
            } else {
                (be32(elst, at)? as u64, be32(elst, at + 4)? as i32 as i64)
            };
            match (media_time, media) {
                (-1, false) => offset += hns(dur, movie_scale),
                (0, false) => media = true,
                _ => return None,
            }
        }
    }
    let stts = child(stbl, b"stts")?;
    let mut times = Vec::new();
    let mut ticks = 0u64;
    for i in 0..be32(stts, 4)? as usize {
        let n = be32(stts, 8 + i * 8)?;
        let delta = be32(stts, 12 + i * 8)? as u64;
        for _ in 0..n {
            times.push(offset + hns(ticks, scale));
            ticks += delta;
        }
    }
    Some(times)
}

fn sample_sizes(stsz: &[u8]) -> Option<Vec<u32>> {
    let fixed = be32(stsz, 4)?;
    let count = be32(stsz, 8)? as usize;
    if fixed != 0 {
        return Some(vec![fixed; count]);
    }
    (0..count).map(|i| be32(stsz, 12 + i * 4)).collect()
}

fn chunk_offsets(stbl: &[u8]) -> Option<Vec<u64>> {
    if let Some(stco) = child(stbl, b"stco") {
        return (0..be32(stco, 4)? as usize).map(|i| be32(stco, 8 + i * 4).map(u64::from)).collect();
    }
    let co64 = child(stbl, b"co64")?;
    (0..be32(co64, 4)? as usize).map(|i| be64(co64, 8 + i * 8)).collect()
}

fn handler(trak: &[u8]) -> Option<[u8; 4]> {
    find(trak, &[b"mdia", b"hdlr"])?.get(8..12)?.try_into().ok()
}

// Timescale de un `mvhd` o `mdhd`, que comparten disposición.
fn timescale(full: &[u8]) -> Option<u64> {
    Some(if *full.first()? == 1 { be32(full, 20)? } else { be32(full, 12)? } as u64)
}

fn hns(ticks: u64, timescale: u64) -> i64 {
    (ticks as u128 * 10_000_000 / timescale as u128) as i64
}

fn children(d: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        let size = be32(d, pos)? as usize;
        let typ: [u8; 4] = d.get(pos + 4..pos + 8)?.try_into().ok()?;
        let (size, header) = match size {
            1 => (usize::try_from(be64(d, pos + 8)?).ok()?, 16),
            0 => (d.len() - pos, 8),
            n => (n, 8),
        };
        let body = d.get(pos + header..pos.checked_add(size)?)?;
        pos += size;
        Some((typ, body))
    })
}

fn child<'a>(d: &'a [u8], typ: &[u8; 4]) -> Option<&'a [u8]> {
    children(d).find(|(t, _)| t == typ).map(|(_, b)| b)
}

fn find<'a>(mut d: &'a [u8], path: &[&[u8; 4]]) -> Option<&'a [u8]> {
    for typ in path {
        d = child(d, typ)?;
    }
    Some(d)
}

fn be16(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn be32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn be64(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(d.get(at..at + 8)?.try_into().ok()?))
}
