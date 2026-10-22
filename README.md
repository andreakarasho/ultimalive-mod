# ultimalive

A ClassicUO mod for UltimaLive shards: the server changes the map (land and statics)
while you play, and the client redraws the changed blocks. It is a port of the
legacy client's built-in `UltimaLive.cs`.

## How it works

The mod watches the incoming packets 0x3F and 0x40. The client has no handler for
these, so the mod also blocks them.

| Packet | Meaning | What the mod does |
|---|---|---|
| 0x3F/02 | Live login | Switches the mod on and re-applies the cached blocks of that shard. Before this, every UltimaLive packet is ignored. |
| 0x3F/01 | Map definitions | Stores the wrap size of each map. |
| 0x3F/FF | Hash query | Replies with the Fletcher-16 CRCs of the 5x5 blocks around the asked block. It reads them from the client's map data (`assets.map-block`, raw land and statics bytes). |
| 0x3F/00 | Statics block | Replaces the block's statics (`assets.set-map-statics`). |
| 0x40 | Land block | Replaces the block's land (`assets.set-map-land`). |

The client keeps patched blocks in memory, as an overlay on its map files. The
world, walking, the pathfinder, the radar and other mods' `map-*` calls all read
from the overlay. The touched chunks reload, and so do their neighbours after a
land change.

## Build

```
make build                       # dist/ultimalive/{mod.wasm,mod.json}
make test                        # protocol tests (native)
```

## Install

Copy `dist/ultimalive` into the client's `Data/Mods` folder.

## Limitations

- **Cache.** Every block the server sends is kept in the mod's folder,
  `shards/<shard>.bin` (`Data/Mods/ultimalive/`; OPFS `mods/ultimalive/` in the
  browser), keyed by the shard name from the live login. At the next live login
  the mod re-applies them, so the server only sends what changed since. The cache
  is saved at the next hash query, so the blocks of a session's last query are
  downloaded again. Delete the file to clear it.
- **No new maps.** 0x3F/01 cannot add maps beyond the client's map count, or
  change map sizes. Only the wrap sizes are used, for the CRC window.
- **Big world map.** The big world map image doesn't redraw patched blocks until
  it is rebuilt. The radar (minimap) does redraw them.
