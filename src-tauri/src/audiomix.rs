// Mezcla de audio por timestamp: cada fuente entrega PCM con el instante QPC en que sonó y aquí se
// coloca en una línea de tiempo común, no por orden de llegada. La usan las pistas por app (varios
// procesos de una misma app) y la pista de mezcla del clip (todas las fuentes más el micro).

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub const RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
pub const CH: usize = CHANNELS as usize;
pub const BLOCK: i64 = 480;
// Dentro de este margen un paquete sigue al anterior sin hueco; la deriva entre el reloj de audio
// y el QPC se corrige de a una muestra, inaudible, en vez de con un salto.
const SLACK: i64 = 48;
const RESYNC: i64 = 4_800;
const MAX_BACKLOG: i64 = RATE as i64 * 5;

// Destino de los bloques ya mezclados: PCM16, instante y duración (100 ns).
pub type Emit<'a> = Box<dyn FnMut(Vec<u8>, i64, i64) + 'a>;

pub struct Mix {
    origin: i64,
    emitted: i64,
    ch: usize,
    acc: VecDeque<i32>,
}

impl Mix {
    pub fn new(origin: i64) -> Self {
        Self::with_channels(origin, CH)
    }

    pub fn with_channels(origin: i64, ch: usize) -> Self {
        Mix { origin, emitted: 0, ch: ch.max(1), acc: VecDeque::new() }
    }

    pub fn frame_at(&self, t: i64) -> i64 {
        ((t - self.origin) as i128 * RATE as i128).div_euclid(10_000_000) as i64
    }

    fn time_of(&self, frame: i64) -> i64 {
        self.origin + (frame as i128 * 10_000_000 / RATE as i128) as i64
    }

    fn add(&mut self, at: i64, pcm: &[i16]) {
        let (mut start, mut src) = (at, pcm);
        if start < self.emitted {
            let skip = (self.emitted - start) as usize * self.ch;
            if skip >= src.len() {
                return;
            }
            src = &src[skip..];
            start = self.emitted;
        }
        if start - self.emitted > MAX_BACKLOG {
            return;
        }
        let off = (start - self.emitted) as usize * self.ch;
        if self.acc.len() < off + src.len() {
            self.acc.resize(off + src.len(), 0);
        }
        for (i, &s) in src.iter().enumerate() {
            self.acc[off + i] += s as i32;
        }
    }

    pub fn drain(&mut self, upto: i64, out: &mut dyn FnMut(Vec<u8>, i64, i64)) {
        // Tras una suspensión o un parón largo del hilo se salta el tramo en vez de soltar
        // minutos de silencio de golpe.
        if upto - self.emitted > MAX_BACKLOG {
            self.skip(upto - BLOCK);
        }
        while self.emitted + BLOCK <= upto {
            let mut bytes = Vec::with_capacity(BLOCK as usize * self.ch * 2);
            for _ in 0..BLOCK as usize * self.ch {
                let v = self.acc.pop_front().unwrap_or(0).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            let t = self.time_of(self.emitted);
            let d = self.time_of(self.emitted + BLOCK) - t;
            self.emitted += BLOCK;
            out(bytes, t, d);
        }
    }

    // Avanza sin entregar nada: una pista sin fuentes no escribe silencio, deja el hueco.
    pub fn skip(&mut self, upto: i64) {
        if upto <= self.emitted {
            return;
        }
        let n = ((upto - self.emitted) as usize * self.ch).min(self.acc.len());
        self.acc.drain(..n);
        self.emitted = upto;
    }
}

#[derive(Default)]
pub struct Cursor(Option<i64>);

impl Cursor {
    pub fn place(&mut self, mix: &mut Mix, stamp: i64, pcm: &[i16]) {
        let ch = mix.ch;
        let frames = (pcm.len() / ch) as i64;
        if frames == 0 {
            return;
        }
        let next = match self.0 {
            Some(p) if (stamp - p).abs() <= RESYNC => p,
            _ => {
                mix.add(stamp, pcm);
                self.0 = Some(stamp + frames);
                return;
            }
        };
        let d = stamp - next;
        if d > SLACK {
            mix.add(next, &pcm[..ch]);
            mix.add(next + 1, pcm);
            self.0 = Some(next + 1 + frames);
        } else if d < -SLACK {
            mix.add(next, &pcm[ch..]);
            self.0 = Some(next + frames - 1);
        } else {
            mix.add(next, pcm);
            self.0 = Some(next + frames);
        }
    }
}

// Mezcla compartida entre hilos: cada fuente (clave propia) empuja desde su hilo de captura y el
// hilo de la pista de mezcla la vacía. El lock solo cubre sumar un bloque de 10 ms.
pub struct MixBus {
    st: Mutex<(Mix, HashMap<u64, Cursor>)>,
}

impl MixBus {
    pub fn new(origin: i64) -> Self {
        MixBus { st: Mutex::new((Mix::new(origin), HashMap::new())) }
    }

    // PCM16 entrelazado de 1 o 2 canales a 48 kHz.
    pub fn push(&self, key: u64, time: i64, pcm: &[i16], channels: usize) {
        let mono;
        let stereo = if channels == 1 {
            mono = pcm.iter().flat_map(|&s| [s, s]).collect::<Vec<_>>();
            &mono[..]
        } else {
            pcm
        };
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        let (mix, cursors) = &mut *st;
        let stamp = mix.frame_at(time);
        cursors.entry(key).or_default().place(mix, stamp, stereo);
    }

    pub fn drain(&self, upto_time: i64, out: &mut dyn FnMut(Vec<u8>, i64, i64)) {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        let upto = st.0.frame_at(upto_time);
        st.0.drain(upto, out);
    }
}

// Instante actual en el mismo reloj que los timestamps de WASAPI (QPC en unidades de 100 ns).
#[cfg(target_os = "windows")]
pub fn now_hns() -> i64 {
    use std::sync::OnceLock;
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    static FREQ: OnceLock<i64> = OnceLock::new();
    let freq = *FREQ.get_or_init(|| {
        let mut f = 0i64;
        let _ = unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1)
    });
    let mut c = 0i64;
    let _ = unsafe { QueryPerformanceCounter(&mut c) };
    (c as i128 * 10_000_000 / freq as i128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(mix: &mut Mix, upto: i64) -> Vec<(Vec<i16>, i64, i64)> {
        let mut out = Vec::new();
        mix.drain(upto, &mut |b: Vec<u8>, t, d| {
            let s = b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
            out.push((s, t, d));
        });
        out
    }

    fn tone(frames: usize, v: i16) -> Vec<i16> {
        vec![v; frames * CH]
    }

    #[test]
    fn silence_fills_the_timeline_and_blocks_are_contiguous() {
        let mut mix = Mix::new(1_000_000);
        let blocks = collect(&mut mix, BLOCK * 3);
        assert_eq!(blocks.len(), 3);
        assert!(blocks.iter().all(|(s, _, _)| s.len() == BLOCK as usize * CH && s.iter().all(|&v| v == 0)));
        assert_eq!(blocks[0].1, 1_000_000);
        assert_eq!(blocks[1].1, blocks[0].1 + blocks[0].2);
        assert_eq!(blocks[0].2, 100_000);
    }

    #[test]
    fn sources_are_summed_by_timestamp_not_arrival_order() {
        let mut mix = Mix::new(0);
        let (mut a, mut b) = (Cursor::default(), Cursor::default());
        b.place(&mut mix, 240, &tone(480, 200));
        a.place(&mut mix, 0, &tone(480, 100));
        let out = collect(&mut mix, BLOCK * 2);
        let first = &out[0].0;
        assert_eq!(first[0], 100);
        assert_eq!(first[240 * CH], 300);
        assert_eq!(out[1].0[0], 200);
        assert_eq!(out[1].0[240 * CH], 0);
    }

    #[test]
    fn a_late_packet_keeps_only_what_is_not_emitted_yet() {
        let mut mix = Mix::new(0);
        let _ = collect(&mut mix, BLOCK);
        let mut c = Cursor::default();
        c.place(&mut mix, 100, &tone(480, 50));
        let out = collect(&mut mix, BLOCK * 2);
        assert_eq!(out[0].0[0], 50);
        assert_eq!(out[0].0[99 * CH], 50);
        assert_eq!(out[0].0[100 * CH], 0);
    }

    #[test]
    fn jittered_packets_stay_contiguous() {
        let mut mix = Mix::new(0);
        let mut c = Cursor::default();
        c.place(&mut mix, 0, &tone(480, 10));
        c.place(&mut mix, 480 + 20, &tone(480, 10));
        c.place(&mut mix, 960 - 15, &tone(480, 10));
        let out = collect(&mut mix, BLOCK * 3);
        assert!(out.iter().flat_map(|(s, _, _)| s.iter()).all(|&v| v == 10));
    }

    #[test]
    fn drift_is_absorbed_one_frame_at_a_time() {
        let mut mix = Mix::new(0);
        let mut c = Cursor::default();
        c.place(&mut mix, 0, &tone(480, 10));
        c.place(&mut mix, 480 + 60, &tone(480, 10));
        assert_eq!(c.0, Some(480 + 1 + 480));
        c.place(&mut mix, 961 - 60, &tone(480, 10));
        assert_eq!(c.0, Some(1440));
        let out = collect(&mut mix, BLOCK * 3);
        assert!(out.iter().flat_map(|(s, _, _)| s.iter()).all(|&v| v == 10));
    }

    #[test]
    fn a_large_jump_resyncs_to_the_timestamp() {
        let mut mix = Mix::new(0);
        let mut c = Cursor::default();
        c.place(&mut mix, 0, &tone(480, 10));
        c.place(&mut mix, 480 + 10_000, &tone(480, 10));
        assert_eq!(c.0, Some(10_960));
    }

    #[test]
    fn a_long_stall_skips_ahead_instead_of_flooding() {
        let mut mix = Mix::new(0);
        let out = collect(&mut mix, MAX_BACKLOG * 3);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn skipping_leaves_a_gap_and_later_audio_lands_in_place() {
        let mut mix = Mix::new(0);
        mix.skip(BLOCK * 4);
        let mut c = Cursor::default();
        c.place(&mut mix, BLOCK * 4, &tone(480, 7));
        let out = collect(&mut mix, BLOCK * 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, 400_000);
        assert!(out[0].0.iter().all(|&v| v == 7));
    }

    #[test]
    fn a_mono_timeline_keeps_one_sample_per_frame() {
        let mut mix = Mix::with_channels(0, 1);
        let mut c = Cursor::default();
        c.place(&mut mix, 0, &[5i16; 480]);
        c.place(&mut mix, 480 + 60, &[5i16; 480]);
        let mut out = Vec::new();
        mix.drain(BLOCK * 2, &mut |b, _, _| out.push(b.len()));
        assert_eq!(out, [480 * 2, 480 * 2]);
    }

    #[test]
    fn loud_sums_saturate() {
        let mut mix = Mix::new(0);
        let (mut a, mut b) = (Cursor::default(), Cursor::default());
        a.place(&mut mix, 0, &tone(480, 30_000));
        b.place(&mut mix, 0, &tone(480, 30_000));
        let out = collect(&mut mix, BLOCK);
        assert_eq!(out[0].0[0], i16::MAX);
    }

    #[test]
    fn the_bus_mixes_mono_and_stereo_sources() {
        let bus = MixBus::new(0);
        let hns = |frames: i64| frames * 10_000_000 / RATE as i64;
        bus.push(1, 0, &vec![100i16; 480], 1);
        bus.push(2, hns(0), &tone(480, 20), 2);
        let mut out = Vec::new();
        bus.drain(hns(BLOCK), &mut |b, _, _| out.extend(b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]))));
        assert_eq!(out.len(), 480 * CH);
        assert!(out.iter().all(|&v| v == 120));
    }
}
