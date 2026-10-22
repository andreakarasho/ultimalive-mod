//! The block cache file round-trips and survives a save cut short.

use cuo_ultimalive::cache::{self, Blocks, Patch};

#[test]
fn round_trips() {
    let mut blocks = Blocks::new();
    blocks.insert((0, 7), Patch { land: Some([0xAB; 192]), statics: Some(vec![1, 2, 3, 4, 5, 6, 7]) });
    blocks.insert((2, 99), Patch { land: None, statics: Some(vec![]) });
    blocks.insert((1, 3), Patch { land: Some([0; 192]), statics: None });

    let data = cache::encode(&blocks);
    assert_eq!(cache::decode(&data), blocks);
    assert!(cache::decode(b"").is_empty());
    assert!(cache::decode(b"junk").is_empty());
    assert!(cache::decode(&data[..data.len() - 3]).len() >= 2);
}

#[test]
fn shard_names_stay_inside_the_folder() {
    assert_eq!(cache::path("My Shard"), "shards/My_Shard.bin");
    assert_eq!(cache::path("../../x"), "shards/______x.bin");
}
