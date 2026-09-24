use super::boxes::Buf;

// Tiempo en unidades de 100 ns (las de Media Foundation) a la escala de la pista. Se convierten
// los instantes absolutos y no las duraciones, para que el redondeo no acumule deriva.
pub fn ticks(t: i64, timescale: u32) -> u64 {
    ((t.max(0) as u128 * timescale as u128 + 5_000_000) / 10_000_000) as u64
}

// Índice de una pista: el instante de cada muestra (la duración de una es la distancia a la
// siguiente, así los huecos no descuadran la línea de tiempo) y los tramos contiguos en disco.
#[derive(Default)]
pub struct Table {
    starts: Vec<u64>,
    sizes: Vec<u32>,
    sync: Vec<u32>,
    chunks: Vec<(u64, u32)>,
    last_dur: u32,
}

impl Table {
    // Devuelve el instante asignado: uno que no avanza se empuja un tick para que ninguna
    // muestra dure 0.
    pub fn push(&mut self, start: u64, size: u32, key: bool, own_dur: u32) -> u64 {
        let start = match self.starts.last() {
            Some(&prev) if start <= prev => prev + 1,
            _ => start,
        };
        self.starts.push(start);
        self.sizes.push(size);
        if key {
            self.sync.push(self.starts.len() as u32);
        }
        self.last_dur = own_dur.max(1);
        start
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn add_chunk(&mut self, offset: u64, samples: u32) {
        if samples > 0 {
            self.chunks.push((offset, samples));
        }
    }

    pub fn first_start(&self) -> u64 {
        self.starts.first().copied().unwrap_or(0)
    }

    pub fn end(&self) -> u64 {
        self.starts.last().map_or(0, |s| s + self.last_dur as u64)
    }

    fn durations(&self) -> impl Iterator<Item = u32> + '_ {
        let deltas = self.starts.windows(2).map(|w| (w[1] - w[0]).min(u32::MAX as u64) as u32);
        deltas.chain((!self.starts.is_empty()).then_some(self.last_dur))
    }

    pub fn write(&self, b: &mut Buf, sync_table: bool) {
        let mut stts: Vec<(u32, u32)> = Vec::new();
        for d in self.durations() {
            match stts.last_mut() {
                Some((n, v)) if *v == d => *n += 1,
                _ => stts.push((1, d)),
            }
        }
        let s = b.open_full(b"stts", 0, 0);
        b.u32(stts.len() as u32);
        for (n, d) in stts {
            b.u32(n).u32(d);
        }
        b.close(s);

        if sync_table {
            let s = b.open_full(b"stss", 0, 0);
            b.u32(self.sync.len() as u32);
            for n in &self.sync {
                b.u32(*n);
            }
            b.close(s);
        }

        let mut stsc: Vec<(u32, u32)> = Vec::new();
        for (i, (_, n)) in self.chunks.iter().enumerate() {
            if stsc.last().map(|(_, c)| c) != Some(n) {
                stsc.push((i as u32 + 1, *n));
            }
        }
        let s = b.open_full(b"stsc", 0, 0);
        b.u32(stsc.len() as u32);
        for (first, n) in stsc {
            b.u32(first).u32(n).u32(1);
        }
        b.close(s);

        let s = b.open_full(b"stsz", 0, 0);
        b.u32(0).u32(self.sizes.len() as u32);
        for size in &self.sizes {
            b.u32(*size);
        }
        b.close(s);

        let wide = self.chunks.iter().any(|(o, _)| *o > u32::MAX as u64);
        let s = b.open_full(if wide { b"co64" } else { b"stco" }, 0, 0);
        b.u32(self.chunks.len() as u32);
        for (o, _) in &self.chunks {
            if wide {
                b.u64(*o);
            } else {
                b.u32(*o as u32);
            }
        }
        b.close(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp4mux::testutil::{be32, be64, find, parse, types};

    fn stbl(t: &Table, sync: bool) -> Vec<u8> {
        let mut b = Buf::default();
        let s = b.open(b"stbl");
        t.write(&mut b, sync);
        b.close(s);
        b.b
    }

    fn entries(d: &[u8], typ: &str, width: usize) -> Vec<Vec<u32>> {
        let tree = parse(d);
        let n = find(&tree, &format!("stbl/{typ}"))[0];
        let count = be32(d, n.body.start + 4) as usize;
        (0..count)
            .map(|i| (0..width).map(|k| be32(d, n.body.start + 8 + (i * width + k) * 4)).collect())
            .collect()
    }

    fn table(starts: &[u64], last_dur: u32) -> Table {
        let mut t = Table::default();
        for (i, s) in starts.iter().enumerate() {
            t.push(*s, 100 + i as u32, i == 0, last_dur);
        }
        t.add_chunk(1000, starts.len() as u32);
        t
    }

    #[test]
    fn ticks_round_to_the_nearest_unit() {
        assert_eq!(ticks(166_667, 90_000), 1_500);
        assert_eq!(ticks(10_000_000, 48_000), 48_000);
        assert_eq!(ticks(694_444, 90_000), 6_250);
    }

    #[test]
    fn durations_come_from_the_next_start() {
        let d = stbl(&table(&[0, 1500, 3000, 4500], 1500), true);
        assert_eq!(entries(&d, "stts", 2), vec![vec![4, 1500]]);
    }

    #[test]
    fn a_gap_stretches_only_that_sample() {
        let d = stbl(&table(&[0, 1500, 4500], 1500), true);
        assert_eq!(entries(&d, "stts", 2), vec![vec![1, 1500], vec![1, 3000], vec![1, 1500]]);
    }

    #[test]
    fn a_start_that_does_not_advance_is_pushed_forward() {
        let mut t = Table::default();
        assert_eq!(t.push(10, 1, true, 5), 10);
        assert_eq!(t.push(10, 1, false, 5), 11);
        assert_eq!(t.push(3, 1, false, 5), 12);
        assert_eq!(t.end(), 17);
    }

    #[test]
    fn span_runs_from_first_start_to_end_of_last_sample() {
        let t = table(&[100, 1600, 3100], 1500);
        assert_eq!(t.first_start(), 100);
        assert_eq!(t.end(), 4600);
    }

    #[test]
    fn chunks_with_equal_counts_share_one_stsc_entry() {
        let mut t = Table::default();
        for i in 0..8 {
            t.push(i * 10, 1, false, 10);
        }
        t.add_chunk(100, 3);
        t.add_chunk(200, 3);
        t.add_chunk(300, 2);
        let d = stbl(&t, false);
        assert_eq!(entries(&d, "stsc", 3), vec![vec![1, 3, 1], vec![3, 2, 1]]);
        assert_eq!(entries(&d, "stco", 1), vec![vec![100], vec![200], vec![300]]);
    }

    #[test]
    fn offsets_past_4gb_switch_to_co64() {
        let mut t = Table::default();
        t.push(0, 1, true, 10);
        t.push(10, 1, false, 10);
        t.add_chunk(100, 1);
        t.add_chunk(5_000_000_000, 1);
        let d = stbl(&t, true);
        let tree = parse(&d);
        assert!(find(&tree, "stbl/stco").is_empty());
        let co64 = find(&tree, "stbl/co64")[0];
        assert_eq!(be32(&d, co64.body.start + 4), 2);
        assert_eq!(be64(&d, co64.body.start + 16), 5_000_000_000);
    }

    #[test]
    fn sizes_and_sync_samples_are_listed() {
        let mut t = Table::default();
        for (i, key) in [true, false, false, true, false].iter().enumerate() {
            t.push(i as u64 * 10, 50 + i as u32, *key, 10);
        }
        t.add_chunk(0, 5);
        let d = stbl(&t, true);
        let tree = parse(&d);
        assert_eq!(types(&tree[0].children), ["stts", "stss", "stsc", "stsz", "stco"]);
        let stsz = find(&tree, "stbl/stsz")[0];
        assert_eq!(be32(&d, stsz.body.start + 8), 5);
        assert_eq!(be32(&d, stsz.body.start + 12 + 4 * 4), 54);
        assert_eq!(entries(&d, "stss", 1), vec![vec![1], vec![4]]);
    }

    #[test]
    fn audio_tables_skip_stss() {
        let d = stbl(&table(&[0, 1024], 1024), false);
        assert!(find(&parse(&d), "stbl/stss").is_empty());
    }
}
