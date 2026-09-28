// Supresión de ruido del micrófono con RNNoise (nnnoiseless, Rust puro) y la prueba de voz de
// Ajustes. Corre en el hilo de la pista del micro, antes del AAC; el vídeo no la ve.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use nnnoiseless::DenoiseState;

const FRAME: usize = DenoiseState::FRAME_SIZE;
pub const RATE: u32 = 48_000;
// RNNoise solo entiende bloques de 10 ms: el bloque que se está llenando más el solape de su
// ventana retrasan la salida 20 ms, que se restan del timestamp para no desincronizar el micro.
const LATENCY: usize = 2 * FRAME;
pub const DELAY_100NS: i64 = LATENCY as i64 * 10_000_000 / RATE as i64;
const MAX_TEST_SECS: usize = 30;

static ENABLED: AtomicBool = AtomicBool::new(false);
static LEVEL: AtomicU32 = AtomicU32::new(100);

pub fn configure(enabled: bool, level: u32) {
    ENABLED.store(enabled, Ordering::SeqCst);
    LEVEL.store(level.min(100), Ordering::SeqCst);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

// RNNoise no tiene intensidad: se mezcla su salida con la señal original alineada, que equivale
// a limitar cuánto puede bajar cada banda. La escala va en dB para que el deslizador se oiga
// lineal (0 = sin efecto, 50 = hasta -20 dB) y 100 es RNNoise sin mezclar.
fn dry_gain(level: u32) -> f32 {
    if level >= 100 {
        0.0
    } else {
        10f32.powf(-(level as f32) * 0.4 / 20.0)
    }
}

struct Channel {
    state: Box<DenoiseState<'static>>,
    input: [f32; FRAME],
    filled: usize,
    // Entrada del bloque anterior: es la que corresponde a la salida de RNNoise (ver LATENCY).
    prev: [f32; FRAME],
    wet: [f32; FRAME],
    out: VecDeque<i16>,
}

impl Channel {
    fn new() -> Self {
        let mut out = VecDeque::with_capacity(2 * FRAME);
        out.extend(std::iter::repeat_n(0, FRAME));
        Channel { state: DenoiseState::new(), input: [0.0; FRAME], filled: 0, prev: [0.0; FRAME], wet: [0.0; FRAME], out }
    }

    fn run(&mut self, dry: f32) {
        self.state.process_frame(&mut self.wet, &self.input);
        let wet = 1.0 - dry;
        for i in 0..FRAME {
            let v = self.wet[i] * wet + self.prev[i] * dry;
            self.out.push_back(v.clamp(i16::MIN as f32, i16::MAX as f32) as i16);
        }
        self.prev = self.input;
        self.filled = 0;
    }
}

pub struct Denoiser {
    channels: Vec<Channel>,
}

impl Denoiser {
    pub fn new(channels: u16) -> Self {
        Denoiser { channels: (0..channels.max(1)).map(|_| Channel::new()).collect() }
    }

    // PCM16 entrelazado a 48 kHz, filtrado en sitio y retrasado LATENCY muestras.
    pub fn process(&mut self, pcm: &mut [u8], level: u32) {
        let dry = dry_gain(level);
        let n = self.channels.len();
        for frame in pcm.chunks_exact_mut(2 * n) {
            for (ch, s) in self.channels.iter_mut().zip(frame.chunks_exact_mut(2)) {
                ch.input[ch.filled] = i16::from_le_bytes([s[0], s[1]]) as f32;
                ch.filled += 1;
                if ch.filled == FRAME {
                    ch.run(dry);
                }
                s.copy_from_slice(&ch.out.pop_front().unwrap_or(0).to_le_bytes());
            }
        }
    }

    pub fn process_live(&mut self, pcm: &mut [u8]) {
        self.process(pcm, LEVEL.load(Ordering::SeqCst));
    }
}

// Prueba de voz: graba el micro elegido tal cual hasta que se pulsa otra vez y lo reproduce con
// el nivel del deslizador. La toma queda en memoria para comparar niveles sin volver a hablar.
#[cfg(target_os = "windows")]
pub use voice::{test_discard, test_record, test_stop_and_play, test_stop_playback};

#[cfg(target_os = "windows")]
mod voice {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub(super) struct Take {
        pub(super) pcm: Vec<u8>,
        pub(super) channels: u16,
    }

    struct TestSink {
        pcm: Mutex<Vec<u8>>,
        cap: usize,
    }

    impl crate::audio::AudioSink for TestSink {
        fn push(&self, data: Vec<u8>, _time: i64, _dur: i64) {
            let mut pcm = self.pcm.lock().unwrap_or_else(|e| e.into_inner());
            let room = self.cap.saturating_sub(pcm.len());
            pcm.extend_from_slice(&data[..data.len().min(room)]);
        }
    }

    struct Test {
        track: Option<(crate::audio::TrackHandle, Arc<TestSink>, u16)>,
        take: Option<Take>,
    }

    static TEST: Mutex<Test> = Mutex::new(Test { track: None, take: None });

    fn test() -> std::sync::MutexGuard<'static, Test> {
        TEST.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn test_record(device: &str) -> Result<(), String> {
        crate::sound::stop();
        let kind = crate::audio::TrackKind::Microphone(device.to_string());
        let (_, ch) = crate::audio::probe_format(&kind).ok_or("No se pudo abrir el micrófono")?;
        let (_, channels) = crate::audio::track_format(ch);
        let sink = Arc::new(TestSink { pcm: Mutex::new(Vec::new()), cap: MAX_TEST_SECS * RATE as usize * 2 * channels as usize });
        let track = crate::audio::spawn_track(kind, crate::audio::Encoding::Pcm, ch, sink.clone(), None, false);
        let mut t = test();
        t.track = Some((track, sink, channels));
        Ok(())
    }

    // Para la grabación y devuelve la duración en ms de lo que va a sonar (0 si no hay toma).
    pub fn test_stop_and_play(level: u32) -> u64 {
        let mut t = test();
        if let Some((mut track, sink, channels)) = t.track.take() {
            track.stop();
            let pcm = std::mem::take(&mut *sink.pcm.lock().unwrap_or_else(|e| e.into_inner()));
            t.take = (!pcm.is_empty()).then_some(Take { pcm, channels });
        }
        let Some(take) = t.take.as_ref() else { return 0 };
        let pcm = render(take, level);
        let ms = pcm.len() as u64 * 1000 / (RATE as u64 * 2 * take.channels as u64);
        crate::sound::play_pcm(pcm, RATE, take.channels);
        ms
    }

    pub fn test_stop_playback() {
        crate::sound::stop();
    }

    pub fn test_discard() {
        let mut t = test();
        t.track = None;
        t.take = None;
        crate::sound::stop();
    }

    pub(super) fn render(take: &Take, level: u32) -> Vec<u8> {
        if level == 0 {
            return take.pcm.clone();
        }
        let lead = LATENCY * 2 * take.channels as usize;
        let mut pcm = take.pcm.clone();
        pcm.resize(pcm.len() + lead, 0);
        Denoiser::new(take.channels).process(&mut pcm, level);
        pcm.drain(..lead);
        pcm
    }
}

#[cfg(not(target_os = "windows"))]
pub fn test_record(_device: &str) -> Result<(), String> {
    Err("Solo en Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn test_stop_and_play(_level: u32) -> u64 {
    0
}

#[cfg(not(target_os = "windows"))]
pub fn test_stop_playback() {}

#[cfg(not(target_os = "windows"))]
pub fn test_discard() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn to_samples(pcm: &[u8]) -> Vec<i16> {
        pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    }

    fn peak(samples: &[i16]) -> usize {
        samples.iter().enumerate().max_by_key(|(_, s)| s.unsigned_abs()).map(|(i, _)| i).unwrap()
    }

    // Sin efecto (nivel 0) la salida es la entrada original retrasada exactamente LATENCY: si
    // RNNoise cambiara su retardo, la mezcla con la señal seca dejaría de estar alineada.
    #[test]
    fn output_is_the_input_delayed_by_the_declared_latency() {
        let mut input = vec![0i16; FRAME * 8];
        input[FRAME * 3 + 17] = 12_000;
        let mut pcm = to_bytes(&input);
        let mut d = Denoiser::new(1);
        for chunk in pcm.chunks_mut(2 * 441) {
            d.process(chunk, 0);
        }
        let out = to_samples(&pcm);
        assert_eq!(peak(&out), FRAME * 3 + 17 + LATENCY);
        assert_eq!(out[FRAME * 3 + 17 + LATENCY], 12_000);
    }

    // Con una señal por debajo de su umbral de silencio RNNoise no aplica ganancias: lo que
    // queda es su retardo puro de análisis/síntesis, que la mezcla supone de un bloque.
    #[test]
    fn rnnoise_delays_its_output_by_one_block() {
        let mut state = DenoiseState::new();
        let mut input = vec![0.0f32; FRAME * 6];
        input[FRAME * 2 + 37] = 0.05;
        let mut out = vec![0.0f32; FRAME * 6];
        for (i, o) in input.chunks_exact(FRAME).zip(out.chunks_exact_mut(FRAME)) {
            state.process_frame(o, i);
        }
        let at = out.iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).map(|(i, _)| i).unwrap();
        assert_eq!(at, FRAME * 2 + 37 + FRAME);
    }

    // La salida propia de RNNoise llega con el mismo retardo que la señal seca con la que se mezcla.
    #[test]
    fn rnnoise_output_is_aligned_with_the_dry_signal() {
        let mut input = vec![0i16; FRAME * 8];
        for (i, s) in input.iter_mut().enumerate().skip(FRAME * 3).take(FRAME) {
            *s = ((i as f32 * 0.07).sin() * 8_000.0) as i16;
        }
        let mut wet = to_bytes(&input);
        Denoiser::new(1).process(&mut wet, 100);
        let wet = to_samples(&wet);
        let energy = |range: std::ops::Range<usize>| wet[range].iter().map(|&s| (s as f64).powi(2)).sum::<f64>();
        let before = energy(FRAME * 3..FRAME * 3 + LATENCY);
        let aligned = energy(FRAME * 3 + LATENCY..FRAME * 4 + LATENCY);
        assert!(aligned > before * 4.0, "aligned={aligned} before={before}");
    }

    #[test]
    fn interleaved_channels_are_filtered_independently() {
        let mut input = vec![0i16; FRAME * 6 * 2];
        input[(FRAME * 2 + 5) * 2 + 1] = 9_000;
        let mut pcm = to_bytes(&input);
        Denoiser::new(2).process(&mut pcm, 0);
        let out = to_samples(&pcm);
        let right: Vec<i16> = out.iter().skip(1).step_by(2).copied().collect();
        let left: Vec<i16> = out.iter().step_by(2).copied().collect();
        assert_eq!(peak(&right), FRAME * 2 + 5 + LATENCY);
        assert!(left.iter().all(|&s| s == 0));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn a_test_take_plays_back_without_the_latency_padding() {
        let mut samples = vec![0i16; FRAME * 4];
        samples[100] = 5_000;
        let take = voice::Take { pcm: to_bytes(&samples), channels: 1 };
        let out = to_samples(&voice::render(&take, 1));
        assert_eq!(out.len(), samples.len());
        assert_eq!(peak(&out), 100);
    }
}
