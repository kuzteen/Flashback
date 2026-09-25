use std::collections::VecDeque;
use std::io::{self, Seek, SeekFrom, Write};

use super::boxes::{mdat_header, Buf};
use super::moov::{final_moov, ftyp, init_moov};
use super::table::{ticks, Table};
use super::{Bytes, Track};

const TFHD_DEFAULT_BASE_IS_MOOF: u32 = 0x02_0000;
const TRUN_DATA_OFFSET: u32 = 0x001;
const TRUN_DURATION: u32 = 0x100;
const TRUN_SIZE: u32 = 0x200;
const TRUN_FLAGS: u32 = 0x400;
const SAMPLE_SYNC: u32 = 0x0200_0000;
const SAMPLE_NON_SYNC: u32 = 0x0101_0000;

struct Pending {
    data: Bytes,
    time: i64,
    dur: i64,
    key: bool,
}

// MP4 "híbrido" (como el de OBS): mientras se graba es un MP4 fragmentado (un `moof`+`mdat` por
// GOP de vídeo), así un cierre inesperado deja un archivo reproducible hasta el último fragmento.
// Al terminar se añade un `moov` normal al final y el `moov` inicial y los `moof` pasan a `free`:
// queda un MP4 estándar sin reescribir los datos.
pub struct Hybrid<W: Write + Seek> {
    w: W,
    tracks: Vec<Track>,
    tables: Vec<Table>,
    pending: Vec<VecDeque<Pending>>,
    moov_at: u64,
    moofs: Vec<u64>,
    committed: u64,
}

impl<W: Write + Seek> Hybrid<W> {
    pub fn new(mut w: W, tracks: Vec<Track>) -> io::Result<Self> {
        let head = ftyp();
        let moov = init_moov(&tracks);
        w.write_all(&head)?;
        w.write_all(&moov)?;
        w.flush()?;
        Ok(Hybrid {
            w,
            tables: tracks.iter().map(|_| Table::default()).collect(),
            pending: tracks.iter().map(|_| VecDeque::new()).collect(),
            tracks,
            moov_at: head.len() as u64,
            moofs: Vec::new(),
            committed: (head.len() + moov.len()) as u64,
        })
    }

    pub fn push(&mut self, track: usize, data: Bytes, time: i64, dur: i64, key: bool) -> io::Result<()> {
        if self.tracks[track].is_video() && key && !self.pending[track].is_empty() {
            self.fragment(Some(time))?;
        }
        self.pending[track].push_back(Pending { data, time, dur, key });
        Ok(())
    }

    // Fin del último fragmento completo: si una escritura falla, recortar el archivo aquí deja
    // un MP4 fragmentado válido.
    pub fn committed_len(&self) -> u64 {
        self.committed
    }

    pub fn fragments(&self) -> usize {
        self.moofs.len()
    }

    pub fn into_writer(self) -> W {
        self.w
    }

    pub fn finish(&mut self) -> io::Result<()> {
        self.fragment(None)?;
        if !self.tables.iter().zip(&self.tracks).any(|(t, tr)| tr.is_video() && t.len() > 0) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "sin fotogramas de vídeo"));
        }
        let moov = final_moov(&self.tracks, &self.tables);
        self.w.seek(SeekFrom::Start(self.committed))?;
        self.w.write_all(&moov)?;
        self.w.flush()?;
        self.committed += moov.len() as u64;
        // El `moov` final ya está escrito: si la app muere a mitad de esto, el primer `moov`
        // sigue siendo el fragmentado y el archivo se reproduce igual.
        for at in std::iter::once(self.moov_at).chain(self.moofs.iter().copied()) {
            self.w.seek(SeekFrom::Start(at + 4))?;
            self.w.write_all(b"free")?;
        }
        self.w.seek(SeekFrom::Start(self.committed))?;
        self.w.flush()
    }

    // Vuelca lo pendiente como un fragmento: todo el vídeo (el GOP que termina) y el audio
    // anterior a `cut` (todo si es el último).
    fn fragment(&mut self, cut: Option<i64>) -> io::Result<()> {
        let mut runs: Vec<(usize, Vec<Pending>, u64)> = Vec::new();
        for track in 0..self.tracks.len() {
            let q = &mut self.pending[track];
            let n = match cut {
                Some(c) if !self.tracks[track].is_video() => q.iter().take_while(|p| p.time < c).count(),
                _ => q.len(),
            };
            if n == 0 {
                continue;
            }
            let taken: Vec<Pending> = q.drain(..n).collect();
            let ts = self.tracks[track].timescale();
            let next = if self.tracks[track].is_video() { cut } else { q.front().map(|p| p.time) };
            runs.push((track, taken, next.map_or(u64::MAX, |t| ticks(t, ts))));
        }
        if runs.is_empty() {
            return Ok(());
        }

        let mut moof = Buf::default();
        let m = moof.open(b"moof");
        let mfhd = moof.open_full(b"mfhd", 0, 0);
        moof.u32(self.moofs.len() as u32 + 1);
        moof.close(mfhd);
        let mut offset_fields = Vec::new();
        let mut run_sizes = Vec::new();
        let mut run_layouts = Vec::new();
        for (track, samples, next) in &runs {
            let t = &self.tracks[*track];
            let ts = t.timescale();
            let tab = &mut self.tables[*track];
            let layouts: Vec<_> = samples.iter().map(|p| t.layout(&p.data)).collect();
            let sizes: Vec<u32> = layouts.iter().map(|l| l.size).collect();
            let starts: Vec<u64> = samples
                .iter()
                .zip(&sizes)
                .map(|(p, s)| tab.push(ticks(p.time, ts), *s, p.key, ticks(p.dur, ts) as u32))
                .collect();
            let last = starts[starts.len() - 1];
            let last_dur = if *next == u64::MAX {
                ticks(samples[samples.len() - 1].dur, ts).max(1)
            } else {
                next.saturating_sub(last).max(1)
            };

            let traf = moof.open(b"traf");
            let tfhd = moof.open_full(b"tfhd", 0, TFHD_DEFAULT_BASE_IS_MOOF);
            moof.u32(*track as u32 + 1);
            moof.close(tfhd);
            let tfdt = moof.open_full(b"tfdt", 1, 0);
            moof.u64(starts[0]);
            moof.close(tfdt);
            let video = t.is_video();
            let flags = TRUN_DATA_OFFSET | TRUN_DURATION | TRUN_SIZE | if video { TRUN_FLAGS } else { 0 };
            let trun = moof.open_full(b"trun", 0, flags);
            moof.u32(samples.len() as u32);
            offset_fields.push(moof.b.len());
            moof.u32(0);
            for (i, (p, size)) in samples.iter().zip(&sizes).enumerate() {
                let dur = starts.get(i + 1).map_or(last_dur, |n| n - starts[i]);
                moof.u32(dur.min(u32::MAX as u64) as u32).u32(*size);
                if video {
                    moof.u32(if p.key { SAMPLE_SYNC } else { SAMPLE_NON_SYNC });
                }
            }
            moof.close(trun);
            moof.close(traf);
            run_sizes.push(sizes.iter().map(|s| *s as u64).sum::<u64>());
            run_layouts.push(layouts);
        }
        moof.close(m);

        let payload: u64 = run_sizes.iter().sum();
        let header = mdat_header(payload);
        let mut data_at = (moof.b.len() + header.len()) as u64;
        for (field, size) in offset_fields.iter().zip(&run_sizes) {
            moof.b[*field..*field + 4].copy_from_slice(&(data_at as u32).to_be_bytes());
            data_at += size;
        }

        let moof_at = self.committed;
        let mut chunk_at = moof_at + (moof.b.len() + header.len()) as u64;
        self.w.seek(SeekFrom::Start(moof_at))?;
        self.w.write_all(&moof.b)?;
        self.w.write_all(&header)?;
        for (((track, samples, _), size), layouts) in runs.iter().zip(&run_sizes).zip(&run_layouts) {
            for (p, layout) in samples.iter().zip(layouts) {
                self.tracks[*track].write_sample(&mut self.w, &p.data, layout)?;
            }
            self.tables[*track].add_chunk(chunk_at, samples.len() as u32);
            chunk_at += size;
        }
        self.w.flush()?;
        self.moofs.push(moof_at);
        self.committed = chunk_at;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4mux::testutil::{be32, be64, find, parse, types, Node};
    use std::io::Cursor;
    use std::sync::Arc;

    const SEQ: [u8; 16] = [0, 0, 0, 1, 0x67, 0x4D, 0x00, 0x1F, 0x95, 0xA8, 0, 0, 0, 1, 0x68, 0xEE];
    const USER_DATA: [u8; 14] = [0, 0, 0xFE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x11, 0x90];
    const FRAME: i64 = 166_667;
    const AAC: i64 = 213_333;

    fn writer() -> Hybrid<Cursor<Vec<u8>>> {
        let tracks =
            vec![Track::h264(64, 64, &SEQ).unwrap(), Track::aac(48_000, 2, 160_000, &USER_DATA).unwrap()];
        Hybrid::new(Cursor::new(Vec::new()), tracks).unwrap()
    }

    fn frame(h: &mut Hybrid<Cursor<Vec<u8>>>, i: usize, gop: usize) {
        let key = i.is_multiple_of(gop);
        let data = vec![0, 0, 0, 1, if key { 0x65 } else { 0x41 }, i as u8, 0xAA];
        h.push(0, Arc::new(data), i as i64 * FRAME, FRAME, key).unwrap();
    }

    fn sound(h: &mut Hybrid<Cursor<Vec<u8>>>, i: usize, from: i64) {
        h.push(1, Arc::new(vec![0x21, i as u8, 0xBB, 0xCC]), from + i as i64 * AAC, AAC, true).unwrap();
    }

    fn bytes(h: &Hybrid<Cursor<Vec<u8>>>) -> Vec<u8> {
        h.w.get_ref().clone()
    }

    fn trun_offset(d: &[u8], moof: &Node, traf: usize) -> usize {
        let trun = &moof.children[1 + traf].children.iter().find(|n| &n.typ == b"trun").unwrap();
        moof.offset + be32(d, trun.body.start + 8) as usize
    }

    #[test]
    fn starts_with_a_fragmented_header() {
        let h = writer();
        let d = bytes(&h);
        let tree = parse(&d);
        assert_eq!(types(&tree), ["ftyp", "moov"]);
        assert_eq!(find(&tree, "moov/mvex/trex").len(), 2);
        assert_eq!(h.committed_len(), d.len() as u64);
    }

    #[test]
    fn a_new_keyframe_closes_the_previous_fragment() {
        let mut h = writer();
        for i in 0..3 {
            frame(&mut h, i, 2);
        }
        let d = bytes(&h);
        let tree = parse(&d);
        assert_eq!(types(&tree), ["ftyp", "moov", "moof", "mdat"]);
        assert_eq!(h.committed_len(), d.len() as u64);
        let moof = &tree[2];
        let o = trun_offset(&d, moof, 0);
        assert_eq!(&d[o..o + 7], &[0, 0, 0, 3, 0x65, 0, 0xAA]);
        let trun = find(&tree, "moof/traf/trun")[0];
        assert_eq!(be32(&d, trun.body.start + 4), 2);
    }

    #[test]
    fn audio_follows_video_inside_the_fragment() {
        let mut h = writer();
        sound(&mut h, 0, 0);
        sound(&mut h, 1, 0);
        for i in 0..3 {
            frame(&mut h, i, 2);
        }
        let d = bytes(&h);
        let tree = parse(&d);
        let o = trun_offset(&d, &tree[2], 1);
        assert_eq!(&d[o..o + 4], &[0x21, 0, 0xBB, 0xCC]);
        assert!(o > trun_offset(&d, &tree[2], 0));
    }

    #[test]
    fn audio_past_the_cut_waits_for_the_next_fragment() {
        let mut h = writer();
        for i in 0..3 {
            sound(&mut h, i, 0);
        }
        for i in 0..3 {
            frame(&mut h, i, 2);
        }
        let d = bytes(&h);
        let tree = parse(&d);
        let truns = find(&tree, "moof/traf/trun");
        assert_eq!(be32(&d, truns[1].body.start + 4), 2);
    }

    #[test]
    fn fragment_decode_time_is_the_track_time() {
        let mut h = writer();
        for i in 0..2 {
            sound(&mut h, i, 400_000);
        }
        for i in 0..5 {
            frame(&mut h, i, 2);
        }
        let d = bytes(&h);
        let tree = parse(&d);
        let tfdt = find(&tree, "moof/traf/tfdt");
        assert_eq!(tfdt.len(), 3);
        assert_eq!(be64(&d, tfdt[1].body.start + 4), 3000);
        assert_eq!(be64(&d, tfdt[2].body.start + 4), 1920);
    }

    #[test]
    fn finishing_turns_it_into_a_regular_mp4() {
        let mut h = writer();
        for i in 0..6 {
            sound(&mut h, i, 0);
        }
        for i in 0..6 {
            frame(&mut h, i, 2);
        }
        h.finish().unwrap();
        assert_eq!(h.committed_len(), bytes(&h).len() as u64);
        let d = h.into_writer().into_inner();
        let tree = parse(&d);
        assert_eq!(types(&tree), ["ftyp", "free", "free", "mdat", "free", "mdat", "free", "mdat", "moov"]);
        assert!(find(&tree, "moov/mvex").is_empty());
        let stco = find(&tree, "moov/trak/mdia/minf/stbl/stco");
        let v1 = be32(&d, stco[0].body.start + 12) as usize;
        assert_eq!(&d[v1..v1 + 7], &[0, 0, 0, 3, 0x65, 2, 0xAA]);
        let a0 = be32(&d, stco[1].body.start + 8) as usize;
        assert_eq!(&d[a0..a0 + 4], &[0x21, 0, 0xBB, 0xCC]);
        let stsz = find(&tree, "moov/trak/mdia/minf/stbl/stsz");
        assert_eq!(be32(&d, stsz[0].body.start + 8), 6);
        assert_eq!(be32(&d, stsz[1].body.start + 8), 6);
    }

    #[test]
    fn finishing_without_frames_fails() {
        let mut h = writer();
        assert!(h.finish().is_err());
        assert_eq!(h.fragments(), 0);
    }
}
