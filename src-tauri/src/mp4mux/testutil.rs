use std::ops::Range;

pub struct Node {
    pub typ: [u8; 4],
    pub offset: usize,
    pub body: Range<usize>,
    pub children: Vec<Node>,
}

const CONTAINERS: [&[u8; 4]; 11] =
    [b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts", b"dinf", b"mvex", b"moof", b"traf", b"udta"];

pub fn be32(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(d[at..at + 4].try_into().unwrap())
}

pub fn be64(d: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(d[at..at + 8].try_into().unwrap())
}

pub fn parse(data: &[u8]) -> Vec<Node> {
    parse_range(data, 0, data.len())
}

fn parse_range(data: &[u8], start: usize, end: usize) -> Vec<Node> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let mut size = be32(data, pos) as usize;
        let mut header = 8;
        if size == 1 {
            size = be64(data, pos + 8) as usize;
            header = 16;
        }
        assert!(size >= header && pos + size <= end, "caja mal formada en {pos}");
        let typ: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap();
        let body = pos + header..pos + size;
        let children = if CONTAINERS.contains(&&typ) {
            parse_range(data, body.start, body.end)
        } else if &typ == b"stsd" {
            parse_range(data, body.start + 8, body.end)
        } else if &typ == b"avc1" {
            parse_range(data, body.start + 78, body.end)
        } else if &typ == b"mp4a" {
            parse_range(data, body.start + 28, body.end)
        } else {
            Vec::new()
        };
        out.push(Node { typ, offset: pos, body, children });
        pos += size;
    }
    assert_eq!(pos, end, "cajas no cubren el rango");
    out
}

pub fn find<'a>(nodes: &'a [Node], path: &str) -> Vec<&'a Node> {
    let mut level: Vec<&Node> = nodes.iter().collect();
    for (i, part) in path.split('/').enumerate() {
        let matched: Vec<&Node> =
            level.into_iter().filter(|n| &n.typ[..] == part.as_bytes()).collect();
        if i + 1 == path.split('/').count() {
            return matched;
        }
        level = matched.iter().flat_map(|n| n.children.iter()).collect();
    }
    Vec::new()
}

pub fn types(nodes: &[Node]) -> Vec<String> {
    nodes.iter().map(|n| String::from_utf8_lossy(&n.typ).into_owned()).collect()
}
