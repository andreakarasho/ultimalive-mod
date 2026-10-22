//! The UltimaLive wire format, ported from the legacy client (`Game/UltimaLive.cs`).
//! Offsets index the full packet (id at 0). Pure: no host calls, unit-tested natively.

/// 64 land cells x (tile u16 + z s8).
pub const LAND_LEN: usize = 192;
/// One static entry: graphic u16, x u8, y u8, z s8, hue u16.
pub const STATIC_LEN: usize = 7;
/// The hash query answers the 5x5 blocks around the asked one.
pub const CRC_COUNT: usize = 25;

/// The legacy client clamps a map definition's size to map 0's default size.
const MAX_MAP_W: u16 = 7168;
const MAX_MAP_H: u16 = 4096;

#[derive(Debug, PartialEq, Eq)]
pub enum Message<'a> {
    /// 0x3F/FF: send the CRCs of the 5x5 blocks around `block`.
    HashQuery { block: u32, map: u8 },
    /// 0x3F/00: replace a block's statics (`statics.len()` is a multiple of 7).
    Statics { block: u32, map: u8, statics: &'a [u8] },
    /// 0x3F/01: per map (id, wrap width, wrap height) in tiles, already clamped.
    MapDefinitions(Vec<(u8, u16, u16)>),
    /// 0x3F/02: the server speaks UltimaLive; the shard name keys the block cache
    /// (empty = UltimaLive off, as in the legacy client).
    LiveLogin(String),
    /// 0x40: replace a block's land.
    Land { block: u32, map: u8, land: &'a [u8; LAND_LEN] },
}

fn u16_be(p: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([p[at], p[at + 1]])
}

fn u32_be(p: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([p[at], p[at + 1], p[at + 2], p[at + 3]])
}

/// None = not an UltimaLive packet, or malformed (the legacy client drops those too).
pub fn parse(p: &[u8]) -> Option<Message<'_>> {
    match *p.first()? {
        0x40 if p.len() >= 201 => Some(Message::Land {
            block: u32_be(p, 1),
            map: p[200],
            land: p[5..5 + LAND_LEN].try_into().ok()?,
        }),
        0x3F if p.len() >= 15 => match p[13] {
            0xFF => Some(Message::HashQuery { block: u32_be(p, 3), map: p[14] }),
            0x00 => {
                let len = (u32_be(p, 7) as usize).checked_mul(STATIC_LEN)?;
                let statics = p.get(15..15usize.checked_add(len)?)?;
                Some(Message::Statics { block: u32_be(p, 3), map: p[14], statics })
            }
            0x01 => {
                // The count field is in 7-byte units of a 9-byte record (legacy: * 7 / 9).
                let maps = (u32_be(p, 7) as u64 * 7 / 9) as usize;
                let body = p.get(15..15usize.checked_add(maps.checked_mul(9)?)?)?;
                Some(Message::MapDefinitions(
                    body.chunks_exact(9)
                        .map(|d| {
                            let (w, h) = (u16_be(d, 1).min(MAX_MAP_W), u16_be(d, 3).min(MAX_MAP_H));
                            (d[0], u16_be(d, 5).min(w), u16_be(d, 7).min(h))
                        })
                        .collect(),
                ))
            }
            0x02 if p.len() >= 43 => Some(Message::LiveLogin(
                p[15..43].iter().take_while(|&&c| c != 0).filter(|c| c.is_ascii_graphic() || **c == b' ').map(|&c| c as char).collect::<String>().trim().to_owned(),
            )),
            _ => None,
        },
        _ => None,
    }
}

pub fn fletcher16(data: &[u8]) -> u16 {
    let (mut a, mut b) = (0u16, 0u16);
    for &d in data {
        a = (a + d as u16) % 255;
        b = (b + a) % 255;
    }
    (b << 8) | a
}

/// A block's CRC: land bytes then statics bytes, as on disk.
pub fn block_crc(land: &[u8], statics: &[u8]) -> u16 {
    let mut all = Vec::with_capacity(land.len() + statics.len());
    all.extend_from_slice(land);
    all.extend_from_slice(statics);
    fletcher16(&all)
}

/// The 25 CRCs around `block` (x-major, like the legacy client), wrapping inside the
/// map's wrap area. `w_blocks` / `h_blocks` = the map size in blocks; `wrap_*` = the
/// wrap size in tiles. Kept byte-for-byte with the legacy client, quirks included
/// (block ids past the wrap use the wrap height), since servers were tested against it.
pub fn crc_window(
    block: u32,
    w_blocks: u32,
    h_blocks: u32,
    wrap_w: u16,
    wrap_h: u16,
    mut crc: impl FnMut(u32) -> u16,
) -> [u16; CRC_COUNT] {
    let blocks = (w_blocks * h_blocks) as i64;
    let (bx, by) = ((block / h_blocks) as i64, (block % h_blocks) as i64);
    let (ww, wh) = ((wrap_w >> 3) as i64, (wrap_h >> 3) as i64);
    let w = if bx < ww { ww } else { w_blocks as i64 };
    let h = if by < wh { wh } else { h_blocks as i64 };

    let mut out = [0u16; CRC_COUNT];
    if w == 0 || h == 0 {
        return out;
    }
    for x in -2i64..=2 {
        let mut xb = (bx + x) % w;
        if xb < 0 && xb > -3 {
            xb += w;
        }
        for y in -2i64..=2 {
            let mut yb = (by + y) % h;
            if yb < 0 {
                yb += h;
            }
            let n = xb * h + yb;
            if (0..blocks).contains(&n) && xb < w && yb < h {
                out[((x + 2) * 5 + y + 2) as usize] = crc(n as u32);
            }
        }
    }
    out
}

/// The 0x3F/FF reply: id, length, block, 6 zero bytes, 0xFF, map, 25 CRCs (BE).
pub fn hash_response(block: u32, map: u8, crcs: &[u16; CRC_COUNT]) -> Vec<u8> {
    let mut p = Vec::with_capacity(65);
    p.push(0x3F);
    p.extend_from_slice(&[0, 0]);
    p.extend_from_slice(&block.to_be_bytes());
    p.extend_from_slice(&[0; 6]);
    p.push(0xFF);
    p.push(map);
    for c in crcs {
        p.extend_from_slice(&c.to_be_bytes());
    }
    let len = p.len() as u16;
    p[1..3].copy_from_slice(&len.to_be_bytes());
    p
}
