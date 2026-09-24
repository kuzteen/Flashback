use super::boxes::Buf;
use super::table::Table;
use super::{Codec, Track};

const MOVIE_TIMESCALE: u32 = 1000;
const MATRIX: [u32; 9] = [0x1_0000, 0, 0, 0, 0x1_0000, 0, 0, 0, 0x4000_0000];

pub fn ftyp() -> Vec<u8> {
    let mut b = Buf::default();
    let s = b.open(b"ftyp");
    b.bytes(b"isom").u32(0x200);
    for brand in [b"isom", b"iso2", b"iso6", b"avc1", b"mp41"] {
        b.bytes(brand);
    }
    b.close(s);
    b.b
}

// `moov` de un MP4 fragmentado: pistas sin muestras y un `trex` por pista; las muestras viajan
// en los `moof`.
pub fn init_moov(tracks: &[Track]) -> Vec<u8> {
    let mut b = Buf::default();
    let moov = b.open(b"moov");
    mvhd(&mut b, 0, tracks.len());
    let empty = Table::default();
    for (i, t) in tracks.iter().enumerate() {
        trak(&mut b, t, i as u32 + 1, &empty, false);
    }
    let mvex = b.open(b"mvex");
    for i in 0..tracks.len() {
        let s = b.open_full(b"trex", 0, 0);
        b.u32(i as u32 + 1).u32(1).u32(0).u32(0).u32(0);
        b.close(s);
    }
    b.close(mvex);
    b.close(moov);
    b.b
}

pub fn final_moov(tracks: &[Track], tables: &[Table]) -> Vec<u8> {
    let mut b = Buf::default();
    let moov = b.open(b"moov");
    let duration = tracks
        .iter()
        .zip(tables)
        .map(|(t, tab)| to_movie(tab.end(), t.timescale()))
        .max()
        .unwrap_or(0);
    mvhd(&mut b, duration, tracks.len());
    for (i, (t, tab)) in tracks.iter().zip(tables).enumerate() {
        trak(&mut b, t, i as u32 + 1, tab, true);
    }
    b.close(moov);
    b.b
}

fn to_movie(t: u64, timescale: u32) -> u64 {
    (t * MOVIE_TIMESCALE as u64 + timescale as u64 / 2) / timescale as u64
}

fn wide(v: u64) -> bool {
    v > u32::MAX as u64
}

fn matrix(b: &mut Buf) {
    for m in MATRIX {
        b.u32(m);
    }
}

fn mvhd(b: &mut Buf, duration: u64, tracks: usize) {
    let v1 = wide(duration);
    let s = b.open_full(b"mvhd", v1 as u8, 0);
    if v1 {
        b.u64(0).u64(0).u32(MOVIE_TIMESCALE).u64(duration);
    } else {
        b.u32(0).u32(0).u32(MOVIE_TIMESCALE).u32(duration as u32);
    }
    b.u32(0x1_0000).u16(0x100).zeros(10);
    matrix(b);
    b.zeros(24).u32(tracks as u32 + 1);
    b.close(s);
}

fn trak(b: &mut Buf, t: &Track, id: u32, tab: &Table, complete: bool) {
    let ts = t.timescale();
    let media = tab.end().saturating_sub(tab.first_start());
    let delay = to_movie(tab.first_start(), ts);
    let presented = delay + to_movie(media, ts);
    let trak = b.open(b"trak");

    let v1 = wide(presented);
    let s = b.open_full(b"tkhd", v1 as u8, 3);
    if v1 {
        b.u64(0).u64(0).u32(id).u32(0).u64(presented);
    } else {
        b.u32(0).u32(0).u32(id).u32(0).u32(presented as u32);
    }
    b.zeros(8).u16(0).u16(0).u16(if t.is_video() { 0 } else { 0x100 }).u16(0);
    matrix(b);
    match t.codec {
        Codec::H264 { width, height, .. } => b.u32(width << 16).u32(height << 16),
        Codec::Aac { .. } => b.u32(0).u32(0),
    };
    b.close(s);

    // El audio del loopback puede empezar más tarde que el vídeo: un tramo vacío lo coloca en
    // su sitio (en el MP4 fragmentado ya lo hace el `tfdt`).
    if complete && delay > 0 {
        let edts = b.open(b"edts");
        let v1 = wide(presented);
        let s = b.open_full(b"elst", v1 as u8, 0);
        b.u32(2);
        let media_movie = to_movie(media, ts);
        if v1 {
            b.u64(delay).u64(u64::MAX).u16(1).u16(0);
            b.u64(media_movie).u64(0).u16(1).u16(0);
        } else {
            b.u32(delay as u32).u32(u32::MAX).u16(1).u16(0);
            b.u32(media_movie as u32).u32(0).u16(1).u16(0);
        }
        b.close(s);
        b.close(edts);
    }

    let mdia = b.open(b"mdia");
    let v1 = wide(media);
    let s = b.open_full(b"mdhd", v1 as u8, 0);
    if v1 {
        b.u64(0).u64(0).u32(ts).u64(media);
    } else {
        b.u32(0).u32(0).u32(ts).u32(media as u32);
    }
    b.u16(0x55C4).u16(0);
    b.close(s);

    let (handler, name): (&[u8; 4], &[u8]) = if t.is_video() {
        (b"vide", b"VideoHandler\0")
    } else {
        (b"soun", b"SoundHandler\0")
    };
    let s = b.open_full(b"hdlr", 0, 0);
    b.u32(0).bytes(handler).zeros(12).bytes(name);
    b.close(s);

    let minf = b.open(b"minf");
    if t.is_video() {
        let s = b.open_full(b"vmhd", 0, 1);
        b.zeros(8);
        b.close(s);
    } else {
        let s = b.open_full(b"smhd", 0, 0);
        b.zeros(4);
        b.close(s);
    }
    let dinf = b.open(b"dinf");
    let dref = b.open_full(b"dref", 0, 0);
    b.u32(1);
    let url = b.open_full(b"url ", 0, 1);
    b.close(url);
    b.close(dref);
    b.close(dinf);

    let stbl = b.open(b"stbl");
    let stsd = b.open_full(b"stsd", 0, 0);
    b.u32(1);
    sample_entry(b, t);
    b.close(stsd);
    tab.write(b, complete && t.is_video());
    b.close(stbl);
    b.close(minf);
    b.close(mdia);
    b.close(trak);
}

fn sample_entry(b: &mut Buf, t: &Track) {
    match &t.codec {
        Codec::H264 { width, height, avcc } => {
            let s = b.open(b"avc1");
            b.zeros(6).u16(1).zeros(16);
            b.u16(*width as u16).u16(*height as u16);
            b.u32(0x48_0000).u32(0x48_0000).u32(0).u16(1).zeros(32).u16(0x18).u16(0xFFFF);
            let c = b.open(b"avcC");
            b.bytes(avcc);
            b.close(c);
            b.close(s);
        }
        Codec::Aac { sample_rate, channels, bitrate, asc } => {
            let s = b.open(b"mp4a");
            b.zeros(6).u16(1).zeros(8);
            b.u16(*channels).u16(16).u16(0).u16(0).u32(sample_rate.min(&0xFFFF) << 16);
            esds(b, asc, *bitrate);
            b.close(s);
        }
    }
}

fn esds(b: &mut Buf, asc: &[u8], bitrate: u32) {
    let dsi = 2 + asc.len();
    let dcd = 13 + dsi;
    let es = 3 + 2 + dcd + 3;
    let s = b.open_full(b"esds", 0, 0);
    b.u8(0x03).u8(es as u8).u16(0).u8(0);
    b.u8(0x04).u8(dcd as u8).u8(0x40).u8(0x15).zeros(3).u32(bitrate).u32(bitrate);
    b.u8(0x05).u8(asc.len() as u8).bytes(asc);
    b.u8(0x06).u8(1).u8(2);
    b.close(s);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4mux::testutil::{be32, find, parse, types, Node};

    const SEQ: [u8; 16] = [0, 0, 0, 1, 0x67, 0x4D, 0x00, 0x1F, 0x95, 0xA8, 0, 0, 0, 1, 0x68, 0xEE];
    const USER_DATA: [u8; 14] = [0, 0, 0xFE, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x11, 0x90];

    fn tracks() -> Vec<Track> {
        vec![
            Track::h264(1280, 720, &SEQ).unwrap(),
            Track::aac(48_000, 2, 160_000, &USER_DATA).unwrap(),
            Track::aac(48_000, 1, 96_000, &USER_DATA).unwrap(),
        ]
    }

    fn body<'a>(d: &'a [u8], n: &Node) -> &'a [u8] {
        &d[n.body.clone()]
    }

    fn table(starts: &[u64], dur: u32) -> Table {
        let mut t = Table::default();
        for s in starts {
            t.push(*s, 10, true, dur);
        }
        t.add_chunk(100, starts.len() as u32);
        t
    }

    #[test]
    fn tracks_need_their_headers() {
        assert!(Track::h264(1280, 720, &[0, 0, 0, 1, 0x67, 0x4D, 0, 0x1F]).is_none());
        assert!(Track::aac(48_000, 2, 160_000, &USER_DATA[..12]).is_none());
        assert_eq!(Track::h264(1280, 720, &SEQ).unwrap().timescale(), 90_000);
        assert_eq!(Track::aac(44_100, 2, 160_000, &USER_DATA).unwrap().timescale(), 44_100);
    }

    #[test]
    fn ftyp_declares_isom() {
        let d = ftyp();
        let tree = parse(&d);
        assert_eq!(types(&tree), ["ftyp"]);
        assert_eq!(&d[8..12], b"isom");
    }

    #[test]
    fn init_moov_is_fragmented_with_one_trex_per_track() {
        let d = init_moov(&tracks());
        let tree = parse(&d);
        assert_eq!(types(&tree[0].children), ["mvhd", "trak", "trak", "trak", "mvex"]);
        let trex = find(&tree, "moov/mvex/trex");
        assert_eq!(trex.len(), 3);
        assert_eq!(be32(&d, trex[2].body.start + 4), 3);
        assert!(find(&tree, "moov/trak/edts").is_empty());
        assert!(find(&tree, "moov/trak/mdia/minf/stbl/stss").is_empty());
    }

    #[test]
    fn sample_entries_carry_codec_config() {
        let d = init_moov(&tracks());
        let tree = parse(&d);
        let avcc = find(&tree, "moov/trak/mdia/minf/stbl/stsd/avc1/avcC")[0];
        assert_eq!(&body(&d, avcc)[..4], &[1, 0x4D, 0x00, 0x1F]);
        let avc1 = find(&tree, "moov/trak/mdia/minf/stbl/stsd/avc1")[0];
        assert_eq!(&d[avc1.body.start + 24..avc1.body.start + 28], &[5, 0, 2, 208]);
        let esds = find(&tree, "moov/trak/mdia/minf/stbl/stsd/mp4a/esds");
        assert_eq!(esds.len(), 2);
        let e = body(&d, esds[0]);
        assert!(e.windows(4).any(|w| w == [0x05, 2, 0x11, 0x90]));
        let mp4a = find(&tree, "moov/trak/mdia/minf/stbl/stsd/mp4a");
        assert_eq!(&d[mp4a[1].body.start + 16..mp4a[1].body.start + 18], &[0, 1]);
        assert_eq!(be32(&d, mp4a[0].body.start + 24), 48_000 << 16);
    }

    #[test]
    fn media_headers_use_track_timescales() {
        let d = init_moov(&tracks());
        let tree = parse(&d);
        let mdhd = find(&tree, "moov/trak/mdia/mdhd");
        assert_eq!(be32(&d, mdhd[0].body.start + 12), 90_000);
        assert_eq!(be32(&d, mdhd[1].body.start + 12), 48_000);
        let hdlr = find(&tree, "moov/trak/mdia/hdlr");
        assert_eq!(&d[hdlr[0].body.start + 8..hdlr[0].body.start + 12], b"vide");
        assert_eq!(&d[hdlr[2].body.start + 8..hdlr[2].body.start + 12], b"soun");
    }

    #[test]
    fn final_moov_has_durations_and_no_mvex() {
        let tables = vec![
            table(&[0, 1500, 3000], 1500),
            table(&[0, 1024, 2048], 1024),
            table(&[0, 1024], 1024),
        ];
        let d = final_moov(&tracks(), &tables);
        let tree = parse(&d);
        assert!(find(&tree, "moov/mvex").is_empty());
        let mdhd = find(&tree, "moov/trak/mdia/mdhd");
        assert_eq!(be32(&d, mdhd[0].body.start + 16), 4500);
        assert_eq!(be32(&d, mdhd[1].body.start + 16), 3072);
        let mvhd = find(&tree, "moov/mvhd")[0];
        assert_eq!(be32(&d, mvhd.body.start + 12), 1000);
        assert_eq!(be32(&d, mvhd.body.start + 16), 64);
        assert_eq!(find(&tree, "moov/trak/mdia/minf/stbl/stss").len(), 1);
    }

    #[test]
    fn delayed_track_gets_an_empty_edit() {
        let tables = vec![
            table(&[0, 1500, 3000], 1500),
            table(&[4800, 5824], 1024),
            table(&[0, 1024], 1024),
        ];
        let d = final_moov(&tracks(), &tables);
        let tree = parse(&d);
        let elst = find(&tree, "moov/trak/edts/elst");
        assert_eq!(elst.len(), 1);
        let e = elst[0].body.start;
        assert_eq!(be32(&d, e + 4), 2);
        assert_eq!(be32(&d, e + 8), 100);
        assert_eq!(be32(&d, e + 12), u32::MAX);
        assert_eq!(be32(&d, e + 20), 43);
        assert_eq!(be32(&d, e + 24), 0);
        let tkhd = find(&tree, "moov/trak/tkhd");
        assert_eq!(be32(&d, tkhd[1].body.start + 20), 143);
    }
}
