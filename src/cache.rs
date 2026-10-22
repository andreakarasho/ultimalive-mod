//! The on-disk copy of the blocks the server sent, so a relog doesn't download them
//! again. One file per shard in the mod's folder (`shards/<shard>.bin`).
//!
//! Format: `ULC1`, then records `kind u8 ('L' land / 'S' statics), map u8, block u32 LE,
//! len u32 LE, bytes`.

use crate::protocol::{LAND_LEN, STATIC_LEN};
use std::collections::HashMap;

const MAGIC: &[u8; 4] = b"ULC1";

/// A block's server copy; `None` halves are still the client's file data.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub land: Option<[u8; LAND_LEN]>,
    pub statics: Option<Vec<u8>>,
}

pub type Blocks = HashMap<(u8, u32), Patch>;

/// The cache file of `shard`, relative to the mod's folder.
pub fn path(shard: &str) -> String {
    let safe: String = shard.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("shards/{safe}.bin")
}

pub fn encode(blocks: &Blocks) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    for (&(map, block), p) in blocks {
        if let Some(land) = &p.land {
            record(&mut out, b'L', map, block, land);
        }
        if let Some(statics) = &p.statics {
            record(&mut out, b'S', map, block, statics);
        }
    }
    out
}

fn record(out: &mut Vec<u8>, kind: u8, map: u8, block: u32, bytes: &[u8]) {
    out.push(kind);
    out.push(map);
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// The stored blocks; a bad record (a save cut short) ends the read.
pub fn decode(data: &[u8]) -> Blocks {
    let mut blocks = Blocks::new();
    let Some(mut rest) = data.strip_prefix(MAGIC) else { return blocks };
    while rest.len() >= 10 {
        let (kind, map) = (rest[0], rest[1]);
        let block = u32::from_le_bytes(rest[2..6].try_into().unwrap());
        let len = u32::from_le_bytes(rest[6..10].try_into().unwrap()) as usize;
        let Some(bytes) = rest.get(10..10 + len) else { break };
        rest = &rest[10 + len..];
        let p = blocks.entry((map, block)).or_default();
        match kind {
            b'L' if len == LAND_LEN => p.land = bytes.try_into().ok(),
            b'S' if len % STATIC_LEN == 0 => p.statics = Some(bytes.to_vec()),
            _ => break,
        }
    }
    blocks.retain(|_, p| p.land.is_some() || p.statics.is_some());
    blocks
}
