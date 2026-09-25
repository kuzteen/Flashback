use std::io::{self, Write};
use std::ops::Range;

use super::boxes::mdat_header;
use super::moov::{final_moov, ftyp};
use super::table::{ticks, Table};
use super::{Packet, Track};

// MP4 con el índice delante (`ftyp → moov → mdat`): todo está en memoria, así que se calculan
// los tamaños sin copiar, se escribe el `moov` con los desplazamientos definitivos y luego los
// datos, convirtiendo el vídeo NAL a NAL. El audio va intercalado por GOP de vídeo.
pub fn write<W: Write>(w: &mut W, tracks: &[Track], packets: &[Vec<Packet>]) -> io::Result<()> {
    let layouts: Vec<Vec<_>> = tracks
        .iter()
        .zip(packets)
        .map(|(t, ps)| ps.iter().map(|p| t.layout(p.data)).collect())
        .collect();
    let sizes: Vec<Vec<u32>> = layouts.iter().map(|l| l.iter().map(|s| s.size).collect()).collect();
    let runs = layout(tracks, packets);
    let payload: u64 = sizes.iter().flatten().map(|s| *s as u64).sum();
    let head = ftyp();
    let mdat = mdat_header(payload);

    // El tamaño del `moov` depende de si los desplazamientos caben en 32 bits (stco/co64) y los
    // desplazamientos del tamaño del `moov`: se repite hasta que se estabiliza (2-3 vueltas).
    let mut moov_len = 0;
    let moov = loop {
        let base = (head.len() + moov_len + mdat.len()) as u64;
        let m = final_moov(tracks, &tables(tracks, packets, &sizes, &runs, base));
        if m.len() == moov_len {
            break m;
        }
        moov_len = m.len();
    };

    w.write_all(&head)?;
    w.write_all(&moov)?;
    w.write_all(&mdat)?;
    for (track, range) in &runs {
        for i in range.clone() {
            tracks[*track].write_sample(w, packets[*track][i].data, &layouts[*track][i])?;
        }
    }
    w.flush()
}

// Tramos contiguos en el `mdat`, en orden: por cada GOP de vídeo, sus fotogramas y después el
// audio anterior al siguiente keyframe.
fn layout(tracks: &[Track], packets: &[Vec<Packet>]) -> Vec<(usize, Range<usize>)> {
    let video = tracks.iter().position(|t| t.is_video());
    let cuts: Vec<(usize, i64)> = match video {
        Some(v) => packets[v]
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, p)| p.key)
            .map(|(i, p)| (i, p.time))
            .collect(),
        None => Vec::new(),
    };
    let mut next = vec![0usize; tracks.len()];
    let mut runs = Vec::new();
    for g in 0..=cuts.len() {
        let limit = cuts.get(g).map(|c| c.1);
        for track in 0..tracks.len() {
            let from = next[track];
            let to = if Some(track) == video {
                cuts.get(g).map_or(packets[track].len(), |c| c.0)
            } else {
                let rest = &packets[track][from..];
                from + limit.map_or(rest.len(), |l| rest.iter().take_while(|p| p.time < l).count())
            };
            if to > from {
                runs.push((track, from..to));
            }
            next[track] = to;
        }
    }
    runs
}

fn tables(
    tracks: &[Track],
    packets: &[Vec<Packet>],
    sizes: &[Vec<u32>],
    runs: &[(usize, Range<usize>)],
    base: u64,
) -> Vec<Table> {
    let mut tabs: Vec<Table> = tracks.iter().map(|_| Table::default()).collect();
    for (track, ps) in packets.iter().enumerate() {
        let ts = tracks[track].timescale();
        for (p, size) in ps.iter().zip(&sizes[track]) {
            tabs[track].push(ticks(p.time, ts), *size, p.key, ticks(p.dur, ts) as u32);
        }
    }
    let mut offset = base;
    for (track, range) in runs {
        tabs[*track].add_chunk(offset, range.len() as u32);
        offset += sizes[*track][range.clone()].iter().map(|s| *s as u64).sum::<u64>();
    }
    tabs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4mux::testutil::{be32, find, parse, types};

    const SEQ: [u8; 16] = [0, 0, 0, 1, 0x67, 0x4D, 0x00, 0x1F, 0x95, 0xA8, 0, 0, 0, 1, 0x68, 0xEE];
    const USER_DATA: [u8; 14] = [0, 0, 0xFE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x11, 0x90];
    const FRAME: i64 = 166_667;
    const AAC: i64 = 213_333;

    fn tracks() -> Vec<Track> {
        vec![Track::h264(64, 64, &SEQ).unwrap(), Track::aac(48_000, 2, 160_000, &USER_DATA).unwrap()]
    }

    fn video(n: usize, gop: usize) -> Vec<(Vec<u8>, i64, bool)> {
        (0..n)
            .map(|i| {
                let key = i % gop == 0;
                let data = vec![0, 0, 0, 1, if key { 0x65 } else { 0x41 }, i as u8, 0xAA];
                (data, i as i64 * FRAME, key)
            })
            .collect()
    }

    fn audio(n: usize, from: i64) -> Vec<(Vec<u8>, i64)> {
        (0..n).map(|i| (vec![0x21, i as u8, 0xBB, 0xCC], from + i as i64 * AAC)).collect()
    }

    fn mux(v: &[(Vec<u8>, i64, bool)], a: &[(Vec<u8>, i64)]) -> Vec<u8> {
        let vp = v.iter().map(|(d, t, k)| Packet { data: d, time: *t, dur: FRAME, key: *k }).collect();
        let ap = a.iter().map(|(d, t)| Packet { data: d, time: *t, dur: AAC, key: true }).collect();
        let mut out = Vec::new();
        write(&mut out, &tracks(), &[vp, ap]).unwrap();
        out
    }

    fn chunk_offsets(d: &[u8], trak: usize) -> Vec<u32> {
        let tree = parse(d);
        let stco = find(&tree, "moov/trak/mdia/minf/stbl/stco")[trak];
        let n = be32(d, stco.body.start + 4) as usize;
        (0..n).map(|i| be32(d, stco.body.start + 8 + i * 4)).collect()
    }

    #[test]
    fn index_comes_before_the_data() {
        let d = mux(&video(4, 2), &audio(3, 0));
        assert_eq!(types(&parse(&d)), ["ftyp", "moov", "mdat"]);
    }

    #[test]
    fn video_chunks_point_at_length_prefixed_frames() {
        let d = mux(&video(4, 2), &audio(3, 0));
        let offs = chunk_offsets(&d, 0);
        assert_eq!(offs.len(), 2);
        let o = offs[1] as usize;
        assert_eq!(&d[o..o + 7], &[0, 0, 0, 3, 0x65, 2, 0xAA]);
    }

    #[test]
    fn audio_is_interleaved_per_gop() {
        let d = mux(&video(4, 2), &audio(3, 0));
        let offs = chunk_offsets(&d, 1);
        assert_eq!(offs.len(), 2);
        assert_eq!(&d[offs[0] as usize..offs[0] as usize + 4], &[0x21, 0, 0xBB, 0xCC]);
        assert_eq!(&d[offs[1] as usize..offs[1] as usize + 4], &[0x21, 2, 0xBB, 0xCC]);
        let v = chunk_offsets(&d, 0);
        assert!(offs[0] > v[0] && offs[0] < v[1]);
    }

    #[test]
    fn keyframes_are_listed_as_sync_samples() {
        let d = mux(&video(6, 3), &audio(2, 0));
        let tree = parse(&d);
        let stss = find(&tree, "moov/trak/mdia/minf/stbl/stss")[0];
        assert_eq!(be32(&d, stss.body.start + 4), 2);
        assert_eq!(be32(&d, stss.body.start + 12), 4);
    }

    #[test]
    fn late_audio_gets_an_edit_list() {
        let d = mux(&video(4, 2), &audio(3, 1_000_000));
        let tree = parse(&d);
        assert_eq!(find(&tree, "moov/trak/edts/elst").len(), 1);
    }
}
