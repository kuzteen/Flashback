use std::fs::{File, OpenOptions};
use std::io::BufWriter;

use super::*;
use crate::mp4mux::hybrid::Hybrid;
use crate::mp4mux::{progressive, Packet, Track};

const WRITE_BUFFER: usize = 1 << 20;

struct MuxSample {
    track: usize,
    data: Bytes,
    time: i64,
    dur: i64,
    key: bool,
}

// Muxer en directo de la grabación manual: recibe paquetes ya codificados (H.264 + AAC) del
// pipeline compartido y los escribe como MP4 híbrido (ver mp4mux::hybrid). Las pistas necesitan
// sus cabeceras al declararse (SPS/PPS del vídeo, AudioSpecificConfig de cada pista), que solo se
// conocen tras el primer paquete de cada fuente: por eso hay un handshake que bufferea hasta
// tenerlas. El disco lo toca un hilo propio; aquí solo se encolan referencias a los paquetes.
struct LiveMuxState {
    tx: Option<mpsc::Sender<MuxSample>>,
    worker: Option<JoinHandle<bool>>,
    // Pista del MP4 de cada pista de audio esperada (None si se descartó).
    tracks: Vec<Option<usize>>,
    base: i64,
    seq_header: Vec<u8>,
    hdrs: Vec<Option<(Vec<u8>, u32)>>,
    pending: Vec<(Option<usize>, Bytes, i64, i64, bool)>,
    writing: bool,
    finalized: bool,
    first_pkt_at: Option<Instant>,
    failed: bool,
    // Continuidad entre segmentos del pipeline (ver new_segment): `shift` se suma a los tiempos
    // de entrada; `max_end` es el final del último paquete aceptado, ya desplazado.
    shift: i64,
    max_end: i64,
    shift_pending: bool,
}

pub(super) struct LiveMux {
    path: String,
    width: u32,
    height: u32,
    // Pistas de audio esperadas, en el orden en que se escriben.
    audio: Vec<audio::TrackSpec>,
    header_timeout: Mutex<Duration>,
    st: Mutex<LiveMuxState>,
}

impl LiveMux {
    pub(super) fn new(
        path: String,
        width: u32,
        height: u32,
        audio: Vec<audio::TrackSpec>,
    ) -> Arc<LiveMux> {
        let n = audio.len();
        Arc::new(LiveMux {
            path,
            width,
            height,
            audio,
            header_timeout: Mutex::new(Duration::from_millis(1000)),
            st: Mutex::new(LiveMuxState {
                tx: None,
                worker: None,
                tracks: vec![None; n],
                base: i64::MIN,
                seq_header: Vec::new(),
                hdrs: vec![None; n],
                pending: Vec::new(),
                writing: false,
                finalized: false,
                first_pkt_at: None,
                failed: false,
                shift: 0,
                max_end: i64::MIN,
                shift_pending: false,
            }),
        })
    }

    pub(super) fn path(&self) -> &str {
        &self.path
    }

    pub(super) fn dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(super) fn audio_specs(&self) -> &[audio::TrackSpec] {
        &self.audio
    }

    // El pipeline que alimenta este muxer se reconstruyó (otra ventana del juego, device perdido)
    // con el mismo tamaño: sus tiempos vuelven a empezar en ~0. El siguiente keyframe se coloca
    // justo tras lo ya escrito, así la grabación sigue en el mismo archivo sin salto atrás. El
    // audio que llegue antes de ese keyframe se descarta (menos de un fotograma de hueco).
    pub(super) fn new_segment(&self) {
        let mut st = self.st.lock_ok();
        if st.max_end == i64::MIN {
            st.base = i64::MIN;
            st.pending.clear();
        } else {
            st.shift_pending = true;
        }
    }

    #[cfg(test)]
    pub(super) fn is_writing(&self) -> bool {
        self.st.lock_ok().writing
    }
    #[cfg(test)]
    pub(super) fn set_header_timeout(&self, d: Duration) {
        *self.header_timeout.lock_ok() = d;
    }
    #[cfg(test)]
    pub(super) fn max_end(&self) -> i64 {
        self.st.lock_ok().max_end
    }

    #[cfg(test)]
    pub(super) fn audio_track_is_none(&self, i: usize) -> bool {
        self.st.lock_ok().tracks[i].is_none()
    }

    pub(super) fn set_audio_header(&self, track: usize, user_data: Vec<u8>, payload_type: u32) {
        if let Some(h) = self.st.lock_ok().hdrs.get_mut(track) {
            *h = Some((user_data, payload_type));
        }
    }

    pub(super) fn push_audio(&self, track: usize, data: Bytes, time: i64, dur: i64) {
        let mut st = self.st.lock_ok();
        if st.finalized || st.failed || st.shift_pending {
            return;
        }
        let time = time + st.shift;
        st.max_end = st.max_end.max(time + dur);
        st.first_pkt_at.get_or_insert_with(Instant::now);
        if st.writing {
            self.write_one(&mut st, Some(track), data, time, dur, false);
        } else {
            st.pending.push((Some(track), data, time, dur, false));
            self.try_begin(&mut st);
        }
    }

    // ¿Están todas las cabeceras esperadas, o venció el timeout (fallback que descarta las
    // pistas de audio que aún no reportaron cabecera, p. ej. un micrófono muerto)?
    fn headers_ready(&self, st: &LiveMuxState) -> bool {
        if st.seq_header.is_empty() {
            return false;
        }
        if st.hdrs.iter().all(|h| h.is_some()) {
            return true;
        }
        let timeout = *self.header_timeout.lock_ok();
        st.first_pkt_at.map(|t| t.elapsed() >= timeout).unwrap_or(false)
    }

    // Arranca el hilo de escritura y vuelca lo pendiente. Requiere base (primer keyframe) y
    // headers_ready().
    fn try_begin(&self, st: &mut LiveMuxState) {
        if st.writing || st.failed || st.base == i64::MIN || !self.headers_ready(st) {
            return;
        }
        let Some(video) = Track::h264(self.width, self.height, &st.seq_header) else {
            log::warn!("grabación manual: cabecera de vídeo sin SPS/PPS; no se puede abrir el MP4");
            st.failed = true;
            st.pending.clear();
            return;
        };
        let mut tracks = vec![video];
        for (i, spec) in self.audio.iter().enumerate() {
            st.tracks[i] = audio_track(spec, &st.hdrs[i]).map(|t| {
                tracks.push(t);
                tracks.len() - 1
            });
        }
        let (tx, rx) = mpsc::channel();
        let path = self.path.clone();
        match std::thread::Builder::new()
            .name("flashback-mux".into())
            .spawn(move || write_hybrid(&path, tracks, rx))
        {
            Ok(worker) => {
                st.tx = Some(tx);
                st.worker = Some(worker);
                st.writing = true;
                let pending = std::mem::take(&mut st.pending);
                for (audio, data, time, dur, key) in pending {
                    self.write_one(st, audio, data, time, dur, key);
                }
            }
            Err(e) => {
                log::warn!("grabación manual: no se pudo crear el hilo del muxer: {e}");
                st.failed = true;
                st.pending.clear();
            }
        }
    }

    // Encola un paquete rebasado a `base`. audio=None => vídeo. Descarta lo anterior a la base
    // (el contenedor no admite tiempos negativos), igual que el muxer del replay.
    fn write_one(
        &self,
        st: &mut LiveMuxState,
        audio: Option<usize>,
        data: Bytes,
        time: i64,
        dur: i64,
        key: bool,
    ) {
        let ts = time - st.base;
        if ts < 0 {
            return;
        }
        let track = match audio {
            None => 0,
            Some(i) => match st.tracks.get(i).copied().flatten() {
                Some(t) => t,
                None => return,
            },
        };
        let Some(tx) = &st.tx else { return };
        if tx.send(MuxSample { track, data, time: ts, dur, key }).is_err() && !st.failed {
            st.failed = true;
            log::warn!("grabación manual: el hilo del muxer terminó; se deja de escribir");
        }
    }

    // Cierra el archivo y devuelve su ruta si quedó un MP4 reproducible (completo, o recortado al
    // último fragmento si falló una escritura).
    pub(super) fn finalize(&self) -> Option<String> {
        let worker = {
            let mut st = self.st.lock_ok();
            if st.finalized {
                return None;
            }
            st.finalized = true;
            st.tx = None;
            st.worker.take()?
        };
        match worker.join() {
            Ok(true) => Some(self.path.clone()),
            Ok(false) => None,
            Err(_) => {
                log::error!("grabación manual: el hilo del muxer terminó con un panic");
                None
            }
        }
    }
}

fn audio_track(spec: &audio::TrackSpec, hdr: &Option<(Vec<u8>, u32)>) -> Option<Track> {
    let (ud, payload_type) = hdr.as_ref()?;
    // El muxer escribe AAC crudo; el encoder se elige con payload 0 (ver build_aac_encoder).
    if *payload_type != 0 {
        log::warn!("grabación manual: pista AAC con framing {payload_type}; se omite");
        return None;
    }
    let track = Track::aac(spec.rate, spec.channels, audio::aac_bitrate(spec.channels), ud)
        .map(|t| t.labeled(&spec.id, &spec.name));
    if track.is_none() {
        log::warn!("grabación manual: pista AAC sin AudioSpecificConfig ({} bytes); se omite", ud.len());
    }
    track
}

// Hilo de escritura de la grabación manual. Devuelve si el archivo quedó reproducible.
fn write_hybrid(path: &str, tracks: Vec<Track>, rx: mpsc::Receiver<MuxSample>) -> bool {
    let file = match OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("grabación manual: no se pudo crear {path}: {e}");
            return false;
        }
    };
    let mut mux = match Hybrid::new(BufWriter::with_capacity(WRITE_BUFFER, file), tracks) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("grabación manual: no se pudo escribir la cabecera del MP4: {e}");
            return false;
        }
    };
    let mut ok = true;
    for s in rx.iter() {
        if let Err(e) = mux.push(s.track, s.data, s.time, s.dur, s.key) {
            log::error!("grabación manual: fallo al escribir: {e}");
            ok = false;
            break;
        }
    }
    if ok {
        match mux.finish() {
            Ok(()) => return true,
            Err(e) => log::error!("grabación manual: no se pudo cerrar el MP4: {e}"),
        }
    }
    keep_written(mux)
}

// Tras un fallo (p. ej. disco lleno) se conserva lo escrito hasta el último fragmento completo,
// que ya es un MP4 fragmentado reproducible. Sin ningún fragmento no hay nada que conservar.
fn keep_written(mux: Hybrid<BufWriter<File>>) -> bool {
    let (committed, fragments) = (mux.committed_len(), mux.fragments());
    let (file, _) = mux.into_writer().into_parts();
    fragments > 0 && file.set_len(committed).is_ok()
}

impl LiveMux {
    pub(super) fn push_video_bytes(&self, data: Bytes, time: i64, dur: i64, key: bool) {
        let mut st = self.st.lock_ok();
        if st.finalized || st.failed {
            return;
        }
        if st.shift_pending {
            if !key {
                return;
            }
            st.shift = st.max_end - time;
            st.shift_pending = false;
        }
        let time = time + st.shift;
        st.max_end = st.max_end.max(time + dur);
        st.first_pkt_at.get_or_insert_with(Instant::now);
        // El primer keyframe fija la base temporal; antes de él se descarta (un MP4 no puede
        // empezar fuera de un IDR, igual que save_replay).
        if st.base == i64::MIN {
            if !key {
                return;
            }
            st.base = time;
        }
        if st.writing {
            self.write_one(&mut st, None, data, time, dur, key);
        } else {
            st.pending.push((None, data, time, dur, key));
            self.try_begin(&mut st);
        }
    }
}

impl VideoPacketSink for LiveMux {
    fn set_seq_header(&self, bytes: Vec<u8>) {
        let mut st = self.st.lock_ok();
        if st.seq_header.is_empty() {
            st.seq_header = bytes;
            self.try_begin(&mut st);
        }
    }
    fn push_video(&self, data: Vec<u8>, time: i64, dur: i64, key: bool) {
        self.push_video_bytes(Arc::new(data), time, dur, key);
    }
}

// Guarda el replay con el índice delante (mp4mux::progressive). Los paquetes ya están en RAM y
// se escriben sin copiarlos; el audio anterior al primer keyframe se descarta.
pub(super) fn mux_replay(
    path: &str,
    packets: &[(Bytes, i64, i64, bool)],
    seq_header: &[u8],
    width: u32,
    height: u32,
    audio: Vec<AudioMuxTrack>,
) -> std::io::Result<()> {
    let video = Track::h264(width, height, seq_header).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "cabecera de vídeo sin SPS/PPS")
    })?;
    let base = packets[0].1;
    let mut tracks = vec![video];
    let mut samples = vec![packets
        .iter()
        .map(|(data, time, dur, key)| Packet { data, time: time - base, dur: *dur, key: *key })
        .collect::<Vec<_>>()];
    for audio in &audio {
        if audio.payload_type != 0 {
            continue;
        }
        let spec = &audio.spec;
        let Some(track) = Track::aac(spec.rate, spec.channels, audio::aac_bitrate(spec.channels), &audio.user_data)
            .map(|t| t.labeled(&spec.id, &spec.name))
        else {
            continue;
        };
        tracks.push(track);
        samples.push(
            audio
                .packets
                .iter()
                .filter(|(_, time, _)| *time >= base)
                .map(|(data, time, dur)| Packet { data, time: time - base, dur: *dur, key: true })
                .collect(),
        );
    }
    let mut w = BufWriter::with_capacity(WRITE_BUFFER, File::create(path)?);
    progressive::write(&mut w, &tracks, &samples)
}
