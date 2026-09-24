#[derive(Default)]
pub struct Buf {
    pub b: Vec<u8>,
}

impl Buf {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.b.push(v);
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.b.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.b.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.b.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.b.extend_from_slice(v);
        self
    }
    pub fn zeros(&mut self, n: usize) -> &mut Self {
        self.b.resize(self.b.len() + n, 0);
        self
    }

    pub fn open(&mut self, typ: &[u8; 4]) -> usize {
        let start = self.b.len();
        self.u32(0).bytes(typ);
        start
    }

    pub fn open_full(&mut self, typ: &[u8; 4], version: u8, flags: u32) -> usize {
        let start = self.open(typ);
        self.u32((version as u32) << 24 | (flags & 0xFF_FFFF));
        start
    }

    pub fn close(&mut self, start: usize) {
        let size = (self.b.len() - start) as u32;
        self.b[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }
}

// Cabecera de un `mdat` de `payload` bytes: 8 bytes, o 16 con `largesize` si no cabe en 32 bits.
pub fn mdat_header(payload: u64) -> Vec<u8> {
    if payload + 8 <= u32::MAX as u64 {
        let mut h = ((payload + 8) as u32).to_be_bytes().to_vec();
        h.extend_from_slice(b"mdat");
        h
    } else {
        let mut h = 1u32.to_be_bytes().to_vec();
        h.extend_from_slice(b"mdat");
        h.extend_from_slice(&(payload + 16).to_be_bytes());
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4mux::testutil::{be32, find, parse};

    #[test]
    fn nested_boxes_get_their_sizes() {
        let mut b = Buf::default();
        let outer = b.open(b"moov");
        let inner = b.open_full(b"mvhd", 1, 3);
        b.u32(7);
        b.close(inner);
        b.close(outer);
        assert_eq!(b.b.len(), 8 + 16);
        assert_eq!(be32(&b.b, 0), 24);
        let tree = parse(&b.b);
        let mvhd = find(&tree, "moov/mvhd")[0];
        assert_eq!(be32(&b.b, mvhd.offset), 16);
        assert_eq!(be32(&b.b, mvhd.body.start), 0x0100_0003);
    }

    #[test]
    fn small_mdat_uses_a_32_bit_header() {
        assert_eq!(mdat_header(10), [0, 0, 0, 18, b'm', b'd', b'a', b't']);
    }

    #[test]
    fn huge_mdat_uses_largesize() {
        let h = mdat_header(5_000_000_000);
        assert_eq!(h.len(), 16);
        assert_eq!(be32(&h, 0), 1);
        assert_eq!(u64::from_be_bytes(h[8..16].try_into().unwrap()), 5_000_000_016);
    }
}
