//! UltimaLive for ClassicUO: the server edits the map while you play.
//!
//! The server asks for the CRCs of the blocks around the player (0x3F/FF); for every
//! block whose CRC differs it sends the new land (0x40) and statics (0x3F/00). The mod
//! answers the CRCs from the client's own map data (`assets.map-block`) and hands the
//! new blocks to the client (`assets.set-map-land` / `set-map-statics`), which redraws them.
//! The mod keeps the server's blocks in its folder, one file per shard, and re-applies
//! them at the next live login, so a relog only downloads what changed since.

pub mod cache;
pub mod protocol;

use cuo_mod_sdk::prelude::*;
use cache::Patch;
use protocol::Message;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Default)]
struct Live {
    /// Set by the live-login (0x3F/02); before it every UltimaLive packet is ignored.
    active: bool,
    /// Map id -> wrap size in tiles (0x3F/01).
    wrap: HashMap<u8, (u16, u16)>,
    /// (map, block) -> CRC.
    crcs: HashMap<(u8, u32), u16>,
    /// The shard `blocks` belong to (0x3F/02).
    shard: String,
    /// Every block the server sent this shard, also the CRC source: a patch lands on the
    /// next frame, so `map-block` may still return the old bytes.
    blocks: cache::Blocks,
    /// `blocks` changed since the last save.
    dirty: bool,
}

thread_local! {
    static LIVE: RefCell<Live> = RefCell::new(Live::default());
}

fn setup(app: &mut App) {
    app.add_packet_observer(PacketDirection::Incoming, &[0x3F, 0x40], on_live_packet);
}

// The client has no handler for these ids: blocking them only keeps them off its
// unknown-packet path.
#[system]
fn on_live_packet(packet: Packet, ctx: Option<Res<GameContext>>) -> Verdict {
    let Some(msg) = protocol::parse(&packet) else { return Verdict::Block };
    let current_map = ctx.as_ref().map(|c| c.map);
    LIVE.with(|live| handle(&mut live.borrow_mut(), msg, current_map));
    Verdict::Block
}

fn handle(live: &mut Live, msg: Message, current_map: Option<i32>) {
    match msg {
        Message::LiveLogin(shard) => {
            live.active = !shard.is_empty();
            if live.active && shard != live.shard {
                load(live, shard);
            }
        }
        _ if !live.active => {}
        Message::MapDefinitions(defs) => {
            live.wrap = defs.into_iter().map(|(map, w, h)| (map, (w, h))).collect();
            live.crcs.clear();
        }
        Message::HashQuery { block, map } => {
            // ponytail: the save waits for the next query (one per few steps), so the
            // blocks of the session's last query aren't stored; the server resends them.
            // The whole cache is rewritten each save: fine to a few MB of patches.
            if live.dirty {
                let path = cache::path(&live.shard);
                let _ = std::fs::create_dir_all("shards");
                if let Err(e) = std::fs::write(&path, cache::encode(&live.blocks)) {
                    host::log(&format!("cache save failed ({path}): {e}"));
                }
                live.dirty = false;
            }
            if current_map != Some(map as i32) {
                return;
            }
            let (Some(&(wrap_w, wrap_h)), Some(size)) = (live.wrap.get(&map), assets::map_size(map)) else { return };
            let (w, h) = (size.width >> 3, size.height >> 3);
            if block >= w * h {
                return;
            }
            let crcs = protocol::crc_window(block, w, h, wrap_w, wrap_h, |n| {
                if let Some(&c) = live.crcs.get(&(map, n)) {
                    return c;
                }
                let c = crc(map, n, live.blocks.get(&(map, n)));
                live.crcs.insert((map, n), c);
                c
            });
            packets::send_to_server(&protocol::hash_response(block, map, &crcs));
        }
        Message::Statics { block, map, statics } => {
            assets::set_map_statics(map, block, statics);
            live.blocks.entry((map, block)).or_default().statics = Some(statics.to_vec());
            live.crcs.remove(&(map, block));
            live.dirty = true;
        }
        Message::Land { block, map, land } => {
            assets::set_map_land(map, block, land);
            live.blocks.entry((map, block)).or_default().land = Some(*land);
            live.crcs.remove(&(map, block));
            live.dirty = true;
        }
    }
}

/// Switches to `shard`'s stored blocks and hands them to the client.
fn load(live: &mut Live, shard: String) {
    live.blocks = std::fs::read(cache::path(&shard)).map(|d| cache::decode(&d)).unwrap_or_default();
    live.shard = shard;
    live.crcs.clear();
    live.dirty = false;
    for (&(map, block), p) in &live.blocks {
        if let Some(land) = &p.land {
            assets::set_map_land(map, block, land);
        }
        if let Some(statics) = &p.statics {
            assets::set_map_statics(map, block, statics);
        }
    }
}

/// A block's CRC: the server's copy where there is one, the client's file data otherwise.
fn crc(map: u8, block: u32, patch: Option<&Patch>) -> u16 {
    let file = || assets::map_block(map, block);
    match patch {
        Some(Patch { land: Some(land), statics: Some(statics) }) => protocol::block_crc(land, statics),
        Some(Patch { land: Some(land), statics: None }) => file().map_or(0, |b| protocol::block_crc(land, &b.statics)),
        Some(Patch { land: None, statics: Some(statics) }) => file().map_or(0, |b| protocol::block_crc(&b.land, statics)),
        _ => file().map_or(0, |b| protocol::block_crc(&b.land, &b.statics)),
    }
}

export_mod!(setup);
