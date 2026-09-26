use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;

use windows::core::HSTRING;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use super::annexb::nal_units;
use super::hybrid::Hybrid;
use super::{progressive, Packet, Track};

const H264: &[u8] = include_bytes!("../../tests/fixtures/tiny.h264");
const ADTS: &[u8] = include_bytes!("../../tests/fixtures/tiny.aac");
const FRAME: i64 = 333_333;
const AAC: i64 = 213_333;

pub(crate) struct Media {
    pub seq: Vec<u8>,
    pub video: Vec<(Vec<u8>, i64, bool)>,
    pub audio: Vec<(Vec<u8>, i64)>,
}

fn with_start_code(nal: &[u8]) -> Vec<u8> {
    let mut v = vec![0, 0, 0, 1];
    v.extend_from_slice(nal);
    v
}

pub(crate) fn media(audio_from: i64) -> Media {
    let mut seq = Vec::new();
    let mut video: Vec<(Vec<u8>, i64, bool)> = Vec::new();
    for nal in nal_units(H264) {
        match nal[0] & 0x1F {
            9 => video.push((Vec::new(), video.len() as i64 * FRAME, false)),
            7 | 8 if video.len() == 1 => seq.extend(with_start_code(nal)),
            t => {
                let au = video.last_mut().unwrap();
                au.0.extend(with_start_code(nal));
                au.2 |= t == 5;
            }
        }
    }
    let mut audio = Vec::new();
    let mut at = 0;
    while at + 7 <= ADTS.len() {
        let len = (((ADTS[at + 3] & 3) as usize) << 11) | ((ADTS[at + 4] as usize) << 3) | (ADTS[at + 5] >> 5) as usize;
        audio.push((ADTS[at + 7..at + len].to_vec(), audio_from + audio.len() as i64 * AAC));
        at += len;
    }
    Media { seq, video, audio }
}

fn tracks(m: &Media) -> Vec<Track> {
    let mut ud = vec![0u8; 12];
    ud.extend_from_slice(&[0x11, 0x90]);
    vec![Track::h264(128, 72, &m.seq).unwrap(), Track::aac(48_000, 2, 64_000, &ud).unwrap()]
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("flashback_mp4mux_{}_{name}.mp4", std::process::id()))
}

pub(crate) struct Read {
    pub video: usize,
    pub audio: usize,
    pub first_audio: i64,
}

// Lee el archivo con el mismo lector que el editor y las miniaturas, decodificando las dos
// pistas: así se valida también `avcC` y `esds`, no solo el contenedor.
pub(crate) fn read(path: &std::path::Path) -> Read {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL).unwrap();
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).unwrap();
        let video_out = MFCreateMediaType().unwrap();
        video_out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).unwrap();
        video_out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).unwrap();
        reader.SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, None, &video_out).unwrap();
        let audio_out = MFCreateMediaType().unwrap();
        audio_out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).unwrap();
        audio_out.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float).unwrap();
        reader.SetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32, None, &audio_out).unwrap();

        let mut out = Read { video: 0, audio: 0, first_audio: -1 };
        for (stream, is_video) in [(MF_SOURCE_READER_FIRST_VIDEO_STREAM, true), (MF_SOURCE_READER_FIRST_AUDIO_STREAM, false)] {
            loop {
                let mut flags = 0u32;
                let mut time = 0i64;
                let mut sample = None;
                reader.ReadSample(stream.0 as u32, 0, None, Some(&mut flags), Some(&mut time), Some(&mut sample)).unwrap();
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                if sample.is_some() {
                    if is_video {
                        out.video += 1;
                    } else {
                        if out.first_audio < 0 {
                            out.first_audio = time;
                        }
                        out.audio += 1;
                    }
                }
            }
        }
        out
    }
}

fn write_progressive(m: &Media, path: &PathBuf) {
    let v = m.video.iter().map(|(d, t, k)| Packet { data: d, time: *t, dur: FRAME, key: *k }).collect();
    let a = m.audio.iter().map(|(d, t)| Packet { data: d, time: *t, dur: AAC, key: true }).collect();
    let mut w = BufWriter::new(std::fs::File::create(path).unwrap());
    progressive::write(&mut w, &tracks(m), &[v, a]).unwrap();
}

fn hybrid(m: &Media, path: &PathBuf) -> Hybrid<BufWriter<std::fs::File>> {
    let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path).unwrap();
    let mut h = Hybrid::new(BufWriter::new(f), tracks(m)).unwrap();
    let mut a = m.audio.iter().peekable();
    for (d, t, k) in &m.video {
        while let Some((ad, at)) = a.next_if(|(_, at)| at <= t) {
            h.push(1, Arc::new(ad.clone()), *at, AAC, true).unwrap();
        }
        h.push(0, Arc::new(d.clone()), *t, FRAME, *k).unwrap();
    }
    for (ad, at) in a {
        h.push(1, Arc::new(ad.clone()), *at, AAC, true).unwrap();
    }
    h
}

#[test]
fn fixture_is_what_the_tests_expect() {
    let m = media(0);
    assert_eq!(m.video.len(), 30);
    assert_eq!(m.video.iter().filter(|v| v.2).count(), 3);
    assert!(m.audio.len() > 40);
}

#[test]
fn media_foundation_decodes_a_progressive_file() {
    let m = media(0);
    let path = temp("progressive");
    write_progressive(&m, &path);
    let r = read(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(r.video, 30);
    assert!(r.audio >= m.audio.len() - 2, "audio {} de {}", r.audio, m.audio.len());
}

#[test]
fn media_foundation_decodes_a_finished_hybrid_file() {
    let m = media(0);
    let path = temp("hybrid");
    let mut h = hybrid(&m, &path);
    h.finish().unwrap();
    let r = read(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(r.video, 30);
    assert!(r.audio >= m.audio.len() - 2, "audio {} de {}", r.audio, m.audio.len());
}

#[test]
fn an_unfinished_hybrid_file_plays_up_to_the_last_fragment() {
    let m = media(0);
    let path = temp("crash");
    drop(hybrid(&m, &path));
    let r = read(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(r.video, 20);
    assert!(r.audio > 20);
}

#[test]
fn media_foundation_honours_the_audio_delay() {
    let m = media(1_000_000);
    let path = temp("delay");
    write_progressive(&m, &path);
    let r = read(&path);
    let _ = std::fs::remove_file(&path);
    assert!((r.first_audio - 1_000_000).abs() < 100_000, "primer audio en {}", r.first_audio);
}

// Tiempos (100 ns) de cada fotograma y de los keyframes tal y como los lista Media Foundation.
fn mf_video_times(path: &std::path::Path) -> (Vec<i64>, Vec<i64>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL).unwrap();
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).unwrap();
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).unwrap();
        reader.SetStreamSelection(stream, true).unwrap();
        let (mut frames, mut keys) = (Vec::new(), Vec::new());
        loop {
            let mut flags = 0u32;
            let mut time = 0i64;
            let mut sample: Option<IMFSample> = None;
            reader.ReadSample(stream, 0, None, Some(&mut flags), Some(&mut time), Some(&mut sample)).unwrap();
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            if let Some(s) = sample {
                frames.push(time);
                if s.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0 {
                    keys.push(time);
                }
            }
        }
        (frames, keys)
    }
}

fn assert_index_matches_media_foundation(path: &PathBuf) {
    let ix = crate::mp4index::Mp4::open(path).and_then(|m| m.video()).expect("índice del moov");
    let (frames, keys) = mf_video_times(path);
    let _ = std::fs::remove_file(path);
    assert_eq!(ix.frames.len(), 30);
    assert_eq!(ix.frames, frames);
    assert_eq!(ix.keyframes, keys);
}

#[test]
fn the_video_index_matches_media_foundation() {
    let mut m = media(0);
    let path = temp("index_progressive");
    write_progressive(&m, &path);
    assert_index_matches_media_foundation(&path);

    // Vídeo que empieza tarde: la edit list lo coloca en su sitio y el índice debe respetarlo.
    for v in &mut m.video {
        v.1 += 500_000;
    }
    let path = temp("index_hybrid");
    let mut h = hybrid(&m, &path);
    h.finish().unwrap();
    drop(h);
    assert_index_matches_media_foundation(&path);
}

#[test]
fn an_unfinished_recording_has_no_video_index() {
    let m = media(0);
    let path = temp("index_crash");
    drop(hybrid(&m, &path));
    let ix = crate::mp4index::Mp4::open(&path).and_then(|m| m.video());
    let _ = std::fs::remove_file(&path);
    assert!(ix.is_none());
}

// Vídeo + dos pistas AAC en el orden en que las escribe la captura (sistema y luego micro), cada una
// desde su instante.
pub(crate) fn two_audio_tracks(name: &str, sys_from: i64, mic_from: i64) -> PathBuf {
    let sys = media(sys_from);
    let mic = media(mic_from);
    let mut t = tracks(&sys);
    t.push(tracks(&mic).remove(1));
    let v = sys.video.iter().map(|(d, t, k)| Packet { data: d, time: *t, dur: FRAME, key: *k }).collect();
    let path = temp(name);
    let mut w = BufWriter::new(std::fs::File::create(&path).unwrap());
    progressive::write(&mut w, &t, &[v, audio_packets(&sys), audio_packets(&mic)]).unwrap();
    path
}

fn audio_packets(m: &Media) -> Vec<Packet<'_>> {
    m.audio.iter().map(|(d, t)| Packet { data: d, time: *t, dur: AAC, key: true }).collect()
}
