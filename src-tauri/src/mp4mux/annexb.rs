use std::io::{self, Write};
use std::ops::Range;

const NAL_SPS: u8 = 7;
const NAL_PPS: u8 = 8;
const NAL_AUD: u8 = 9;

// Recorre cada byte de cada paquete (dos veces al guardar un replay: tamaños y escritura), así que
// es el bucle más caliente del muxer. Un código de inicio lleva dos ceros seguidos y el vídeo
// comprimido casi no tiene ceros: se leen 8 bytes de golpe y se salta el bloque entero si no hay
// ninguno, como hace ffmpeg. Byte a byte, un replay de un minuto tardaba segundos en escribirse.
fn start_code(data: &[u8], from: usize) -> Option<usize> {
    const LOW: u64 = 0x0101_0101_0101_0101;
    const HIGH: u64 = 0x8080_8080_8080_8080;
    let mut i = from;
    while i + 3 <= data.len() {
        if let Some(block) = data.get(i..i + 8) {
            let w = u64::from_le_bytes(block.try_into().unwrap_or_default());
            if w.wrapping_sub(LOW) & !w & HIGH == 0 {
                i += 8;
                continue;
            }
        }
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub fn nal_units(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut next = start_code(data, 0).map(|p| p + 3);
    std::iter::from_fn(move || loop {
        let begin = next?;
        let end = start_code(data, begin);
        next = end.map(|p| p + 3);
        let mut nal = &data[begin..end.unwrap_or(data.len())];
        while let [rest @ .., 0] = nal {
            nal = rest;
        }
        if !nal.is_empty() {
            return Some(nal);
        }
    })
}

fn nal_type(nal: &[u8]) -> u8 {
    nal[0] & 0x1F
}

// SPS/PPS viajan en `avcC`; repetirlos en banda (y los AUD) solo ocupa sitio.
fn samples_nals(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    nal_units(data).filter(|n| !matches!(nal_type(n), NAL_SPS | NAL_PPS | NAL_AUD))
}

// NAL que van al MP4, como rangos del paquete. Se buscan una sola vez: el índice necesita el
// tamaño antes de escribir los datos, y recorrer el paquete de nuevo al escribirlo duplicaba el
// coste del muxer.
pub fn mp4_nals(data: &[u8]) -> Vec<Range<usize>> {
    let base = data.as_ptr() as usize;
    samples_nals(data)
        .map(|n| {
            let at = n.as_ptr() as usize - base;
            at..at + n.len()
        })
        .collect()
}

pub fn mp4_size(nals: &[Range<usize>]) -> u32 {
    nals.iter().map(|n| 4 + n.len() as u32).sum()
}

pub fn write_mp4<W: Write>(w: &mut W, data: &[u8], nals: &[Range<usize>]) -> io::Result<()> {
    for n in nals {
        w.write_all(&(n.len() as u32).to_be_bytes())?;
        w.write_all(&data[n.clone()])?;
    }
    Ok(())
}

pub fn avc_config(seq_header: &[u8]) -> Option<Vec<u8>> {
    let sps: Vec<&[u8]> = nal_units(seq_header).filter(|n| nal_type(n) == NAL_SPS).collect();
    let pps: Vec<&[u8]> = nal_units(seq_header).filter(|n| nal_type(n) == NAL_PPS).collect();
    let first = sps.first().filter(|s| s.len() >= 4)?;
    if pps.is_empty() {
        return None;
    }
    let profile = first[1];
    let mut out = vec![1, profile, first[2], first[3], 0xFF, 0xE0 | sps.len() as u8];
    for s in &sps {
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s);
    }
    out.push(pps.len() as u8);
    for p in &pps {
        out.extend_from_slice(&(p.len() as u16).to_be_bytes());
        out.extend_from_slice(p);
    }
    if matches!(profile, 100 | 110 | 122 | 144) {
        let (chroma, luma_depth, chroma_depth) = high_profile_format(first)?;
        out.extend_from_slice(&[0xFC | chroma, 0xF8 | luma_depth, 0xF8 | chroma_depth, 0]);
    }
    Some(out)
}

// chroma_format_idc y profundidades de bit del SPS de un perfil High: ISO 14496-15 exige
// repetirlos al final del `avcC` para esos perfiles.
fn high_profile_format(sps: &[u8]) -> Option<(u8, u8, u8)> {
    let rbsp = unescape(&sps[4..]);
    let mut bits = Bits { data: &rbsp, pos: 0 };
    bits.ue()?;
    let chroma = bits.ue()?;
    if chroma == 3 {
        bits.bit()?;
    }
    let luma_depth = bits.ue()?;
    let chroma_depth = bits.ue()?;
    Some((chroma as u8 & 3, luma_depth as u8 & 7, chroma_depth as u8 & 7))
}

fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.data.get(self.pos / 8)?;
        let b = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Some(b as u32)
    }

    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let mut v = 0u32;
        for _ in 0..zeros {
            v = (v << 1) | self.bit()?;
        }
        Some((1u32 << zeros) - 1 + v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HIGH_SPS: [u8; 27] = [
        0x67, 0x64, 0x00, 0x28, 0xAC, 0xD9, 0x40, 0x78, 0x02, 0x27, 0xE5, 0xC0, 0x44, 0x00, 0x00,
        0x03, 0x00, 0x04, 0x00, 0x00, 0x03, 0x00, 0xF0, 0x3C, 0x60, 0xC6, 0x58,
    ];
    const PPS: [u8; 4] = [0x68, 0xEE, 0x3C, 0x80];

    fn annexb(nals: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for n in nals {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(n);
        }
        out
    }

    fn naive_start_code(data: &[u8], from: usize) -> Option<usize> {
        data.get(from..)?.windows(3).position(|w| w == [0, 0, 1]).map(|p| from + p)
    }

    #[test]
    fn the_block_skip_finds_the_same_start_codes_as_a_byte_scan() {
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..400 {
            let len = (next() % 200) as usize;
            let data: Vec<u8> = (0..len).map(|_| match next() % 8 { 0..=2 => 0, 3 => 1, _ => next() as u8 }).collect();
            for from in 0..=len {
                assert_eq!(start_code(&data, from), naive_start_code(&data, from), "{data:?} desde {from}");
            }
        }
    }

    #[test]
    fn splits_three_and_four_byte_start_codes() {
        let data = [0, 0, 0, 1, 0x09, 0xF0, 0, 0, 1, 0x65, 0xAA, 0xBB, 0, 0, 0, 1, 0x41, 0xCC];
        let nals: Vec<&[u8]> = nal_units(&data).collect();
        assert_eq!(nals, vec![&[0x09, 0xF0][..], &[0x65, 0xAA, 0xBB], &[0x41, 0xCC]]);
    }

    #[test]
    fn trailing_zeros_are_not_part_of_the_nal() {
        let data = [0, 0, 1, 0x65, 0xAA, 0, 0, 0, 1, 0x41];
        let nals: Vec<&[u8]> = nal_units(&data).collect();
        assert_eq!(nals, vec![&[0x65, 0xAA][..], &[0x41]]);
    }

    #[test]
    fn size_drops_parameter_sets_and_delimiters() {
        let data = annexb(&[&[0x09, 0xF0], &HIGH_SPS, &PPS, &[0x65, 0xAA, 0xBB]]);
        assert_eq!(mp4_size(&mp4_nals(&data)), 4 + 3);
    }

    #[test]
    fn writes_length_prefixed_nals() {
        let data = annexb(&[&[0x09, 0xF0], &[0x06, 0x05], &[0x65, 0xAA, 0xBB]]);
        let nals = mp4_nals(&data);
        let mut out = Vec::new();
        write_mp4(&mut out, &data, &nals).unwrap();
        assert_eq!(out, [0, 0, 0, 2, 0x06, 0x05, 0, 0, 0, 3, 0x65, 0xAA, 0xBB]);
        assert_eq!(out.len() as u32, mp4_size(&nals));
    }

    #[test]
    fn avc_config_for_high_profile_carries_chroma_and_depth() {
        let cfg = avc_config(&annexb(&[&HIGH_SPS, &PPS])).unwrap();
        let mut want = vec![1, 0x64, 0x00, 0x28, 0xFF, 0xE1, 0, 27];
        want.extend_from_slice(&HIGH_SPS);
        want.extend_from_slice(&[1, 0, 4]);
        want.extend_from_slice(&PPS);
        want.extend_from_slice(&[0xFD, 0xF8, 0xF8, 0]);
        assert_eq!(cfg, want);
    }

    #[test]
    fn avc_config_for_main_profile_has_no_extension() {
        let sps = [0x67, 0x4D, 0x00, 0x1F, 0x95, 0xA8];
        let cfg = avc_config(&annexb(&[&sps, &PPS])).unwrap();
        assert_eq!(cfg.len(), 6 + 2 + sps.len() + 1 + 2 + PPS.len());
        assert_eq!(&cfg[..4], &[1, 0x4D, 0x00, 0x1F]);
    }

    #[test]
    fn avc_config_needs_sps_and_pps() {
        assert_eq!(avc_config(&annexb(&[&HIGH_SPS])), None);
        assert_eq!(avc_config(&[]), None);
    }
}
