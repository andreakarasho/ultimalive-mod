//! The wire format against hand-built packets in the legacy client's layout.

use cuo_ultimalive::protocol::{self, Message, CRC_COUNT, LAND_LEN};

fn live(sub: u8, block: u32, count: u32, map: u8, body: &[u8]) -> Vec<u8> {
    let mut p = vec![0x3F, 0, 0];
    p.extend_from_slice(&block.to_be_bytes());
    p.extend_from_slice(&count.to_be_bytes());
    p.extend_from_slice(&[0, 0]);
    p.push(sub);
    p.push(map);
    p.extend_from_slice(body);
    let len = p.len() as u16;
    p[1..3].copy_from_slice(&len.to_be_bytes());
    p
}

#[test]
fn fletcher16_known_vectors() {
    assert_eq!(protocol::fletcher16(b"abcde"), 0xC8F0);
    assert_eq!(protocol::fletcher16(b"abcdef"), 0x2057);
    assert_eq!(protocol::fletcher16(b"abcdefgh"), 0x0627);
    assert_eq!(protocol::block_crc(b"abc", b"def"), 0x2057);
}

#[test]
fn parses_every_message() {
    assert_eq!(protocol::parse(&live(0xFF, 1234, 0, 1, &[])), Some(Message::HashQuery { block: 1234, map: 1 }));

    let statics = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
    assert_eq!(
        protocol::parse(&live(0x00, 77, 2, 0, &statics)),
        Some(Message::Statics { block: 77, map: 0, statics: &statics })
    );
    // Truncated statics are dropped, not read past the end.
    assert_eq!(protocol::parse(&live(0x00, 77, 3, 0, &statics)), None);

    // Two maps: the count field is in 7-byte units (rounded up), 9 bytes per map.
    let mut defs = Vec::new();
    for (id, w, h, ww, wh) in [(0u8, 7168u16, 4096u16, 5120u16, 4096u16), (5, 9000, 9000, 9000, 100)] {
        defs.push(id);
        for v in [w, h, ww, wh] {
            defs.extend_from_slice(&v.to_be_bytes());
        }
    }
    assert_eq!(
        protocol::parse(&live(0x01, 0, 3, 0, &defs)),
        Some(Message::MapDefinitions(vec![(0, 5120, 4096), (5, 7168, 100)]))
    );

    assert_eq!(protocol::parse(&live(0x02, 0, 0, 0, &[b'x'; 28])), Some(Message::LiveLogin("x".repeat(28))));
    let mut name = *b"My Shard\n\0trailing junk.......";
    name[27] = 0;
    assert_eq!(protocol::parse(&live(0x02, 0, 0, 0, &name[..28])), Some(Message::LiveLogin("My Shard".into())));
    assert_eq!(protocol::parse(&live(0x02, 0, 0, 0, &[b'x'; 4])), None);

    let mut land = vec![0x40];
    land.extend_from_slice(&4321u32.to_be_bytes());
    land.extend((0..LAND_LEN).map(|i| i as u8));
    land.extend_from_slice(&[0, 0, 0, 2]);
    assert_eq!(land.len(), 201);
    match protocol::parse(&land) {
        Some(Message::Land { block: 4321, map: 2, land: cells }) => assert_eq!(cells[191], 191),
        other => panic!("{other:?}"),
    }
}

#[test]
fn crc_window_is_x_major_and_wraps_at_the_wrap_area() {
    // 10x10 blocks, wrap = the whole map: block (0,0) wraps left/up to 8 and 9.
    let w = protocol::crc_window(0, 10, 10, 80, 80, |n| n as u16);
    let expect = |x: i32, y: i32| (((x + 10) % 10) * 10 + (y + 10) % 10) as u16;
    for x in -2..=2 {
        for y in -2..=2 {
            assert_eq!(w[((x + 2) * 5 + y + 2) as usize], expect(x, y), "({x},{y})");
        }
    }

    // A block in the middle: no wrapping, plain neighbours.
    let mid = protocol::crc_window(55, 10, 10, 80, 80, |n| n as u16);
    assert_eq!(mid[12], 55);
    assert_eq!(mid[0], 33);
    assert_eq!(mid[24], 77);
}

#[test]
fn hash_response_layout() {
    let crcs: [u16; CRC_COUNT] = core::array::from_fn(|i| i as u16 + 0x100);
    let p = protocol::hash_response(0x01020304, 3, &crcs);
    assert_eq!(p.len(), 65);
    assert_eq!(&p[..3], &[0x3F, 0, 65]);
    assert_eq!(&p[3..7], &[1, 2, 3, 4]);
    assert_eq!(&p[7..13], &[0; 6]);
    assert_eq!(&p[13..15], &[0xFF, 3]);
    assert_eq!(&p[15..17], &[0x01, 0x00]);
    assert_eq!(&p[63..65], &[0x01, 0x18]);
}
