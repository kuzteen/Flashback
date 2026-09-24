mod annexb;
mod boxes;
pub mod hybrid;
#[cfg(all(test, windows))]
pub(crate) mod mf_tests;
mod moov;
pub mod progressive;
mod table;
#[cfg(test)]
mod testutil;

pub type Bytes = std::sync::Arc<Vec<u8>>;

// Paquete ya codificado. Tiempos en unidades de 100 ns relativos al origen del archivo (el
// primer keyframe de vídeo). El vídeo llega en Annex B; el audio es AAC crudo.
pub struct Packet<'a> {
    pub data: &'a [u8],
    pub time: i64,
    pub dur: i64,
    pub key: bool,
}

pub enum Codec {
    H264 { width: u32, height: u32, avcc: Vec<u8> },
    Aac { sample_rate: u32, channels: u16, bitrate: u32, asc: Vec<u8> },
}

pub struct Track {
    pub codec: Codec,
}

const VIDEO_TIMESCALE: u32 = 90_000;
// MF_MT_USER_DATA de un tipo AAC = cola de HEAACWAVEINFO (12 bytes) + AudioSpecificConfig.
const HEAAC_INFO_LEN: usize = 12;

impl Track {
    pub fn h264(width: u32, height: u32, seq_header: &[u8]) -> Option<Track> {
        let avcc = annexb::avc_config(seq_header)?;
        Some(Track { codec: Codec::H264 { width, height, avcc } })
    }

    pub fn aac(sample_rate: u32, channels: u16, bitrate: u32, user_data: &[u8]) -> Option<Track> {
        let asc = user_data.get(HEAAC_INFO_LEN..).filter(|a| !a.is_empty())?.to_vec();
        Some(Track { codec: Codec::Aac { sample_rate, channels, bitrate, asc } })
    }

    pub fn timescale(&self) -> u32 {
        match self.codec {
            Codec::H264 { .. } => VIDEO_TIMESCALE,
            Codec::Aac { sample_rate, .. } => sample_rate,
        }
    }

    pub fn is_video(&self) -> bool {
        matches!(self.codec, Codec::H264 { .. })
    }

    fn sample_size(&self, data: &[u8]) -> u32 {
        if self.is_video() {
            annexb::mp4_size(data)
        } else {
            data.len() as u32
        }
    }

    fn write_sample<W: std::io::Write>(&self, w: &mut W, data: &[u8]) -> std::io::Result<()> {
        if self.is_video() {
            annexb::write_mp4(w, data)
        } else {
            w.write_all(data)
        }
    }
}
