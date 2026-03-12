//! Direct unit tests for tracker.rs data structures.
//!
//! The WhiteoutTracker, EmittedPathTracker, and HardLinkTracker are exercised
//! indirectly through integration tests, but these unit tests verify their
//! internal behaviour in isolation — especially edge cases around trie
//! traversal, suppression ordering, and hardlink promotion bookkeeping.

use std::path::Path;

use oci2squashfs::tracker::{EmittedPathTracker, HardLinkTracker, WhiteoutTracker};

// ─── WhiteoutTracker ────────────────────────────────────────────────────────

#[test]
fn whiteout_simple_suppresses_exact_path_from_older_layer() {
    let mut wt = WhiteoutTracker::default();
    wt.insert_simple(Path::new("usr/share/foo"), 2);

    // Layer 1 (older than 2) should be suppressed.
    assert!(wt.is_suppressed(Path::new("usr/share/foo"), 1));
    // Layer 0 (older than 2) should be suppressed.
    assert!(wt.is_suppressed(Path::new("usr/share/foo"), 0));
}

#[test]
fn whiteout_simple_does_not_suppress_same_or_newer_layer() {
    let mut wt = WhiteoutTracker::default();
    wt.insert_simple(Path::new("usr/share/foo"), 2);

    // Layer 2 (same layer) should not be suppressed.
    assert!(!wt.is_suppressed(Path::new("usr/share/foo"), 2));
    // Layer 3 (newer) should not be suppressed.
    assert!(!wt.is_suppressed(Path::new("usr/share/foo"), 3));
}

#[test]
fn whiteout_simple_suppresses_children() {
    // A simple whiteout on a directory should suppress all descendants.
    let mut wt = WhiteoutTracker::default();
    wt.insert_simple(Path::new("home/ubuntu"), 3);

    assert!(wt.is_suppressed(Path::new("home/ubuntu"), 1));
    assert!(wt.is_suppressed(Path::new("home/ubuntu/.bashrc"), 1));
    assert!(wt.is_suppressed(Path::new("home/ubuntu/subdir/file"), 0));
}

#[test]
fn whiteout_simple_does_not_suppress_siblings() {
    let mut wt = WhiteoutTracker::default();
    wt.insert_simple(Path::new("usr/share/foo"), 2);

    assert!(!wt.is_suppressed(Path::new("usr/share/bar"), 0));
    assert!(!wt.is_suppressed(Path::new("usr/share"), 0));
    assert!(!wt.is_suppressed(Path::new("usr"), 0));
}

#[test]
fn whiteout_opaque_suppresses_children_from_older_layers() {
    let mut wt = WhiteoutTracker::default();
    wt.insert_opaque(Path::new("etc"), 2);

    assert!(wt.is_suppressed(Path::new("etc/passwd"), 1));
    assert!(wt.is_suppressed(Path::new("etc/sub/deep"), 0));
}

#[test]
fn whiteout_opaque_does_not_suppress_directory_itself() {
    // The opaque marker suppresses *children*, not the directory entry itself.
    // The is_suppressed check for the directory itself should test the exact
    // node, which is Opaque — and this should suppress, since the path is
    // the directory (the same node).
    let mut wt = WhiteoutTracker::default();
    wt.insert_opaque(Path::new("etc"), 2);

    // The directory from an older layer IS suppressed because the whiteout
    // is set directly on the "etc" node.
    assert!(wt.is_suppressed(Path::new("etc"), 1));
    // But from the same or newer layer, it is NOT suppressed.
    assert!(!wt.is_suppressed(Path::new("etc"), 2));
    assert!(!wt.is_suppressed(Path::new("etc"), 3));
}

#[test]
fn whiteout_unrelated_paths_not_suppressed() {
    let mut wt = WhiteoutTracker::default();
    wt.insert_simple(Path::new("a/b/c"), 1);

    assert!(!wt.is_suppressed(Path::new("x/y/z"), 0));
    assert!(!wt.is_suppressed(Path::new("a/b/d"), 0));
    assert!(!wt.is_suppressed(Path::new("a/b"), 0));
}

#[test]
fn whiteout_empty_tracker_suppresses_nothing() {
    let wt = WhiteoutTracker::default();
    assert!(!wt.is_suppressed(Path::new("foo"), 0));
    assert!(!wt.is_suppressed(Path::new("a/b/c"), 0));
}

#[test]
fn whiteout_multiple_whiteouts_coexist() {
    let mut wt = WhiteoutTracker::default();
    wt.insert_simple(Path::new("a"), 2);
    wt.insert_simple(Path::new("b"), 3);

    assert!(wt.is_suppressed(Path::new("a"), 1));
    assert!(!wt.is_suppressed(Path::new("a"), 2));
    assert!(wt.is_suppressed(Path::new("b"), 2));
    assert!(!wt.is_suppressed(Path::new("b"), 3));
}

#[test]
fn whiteout_simple_then_opaque_on_ancestor() {
    // Simple whiteout on a file, opaque on its parent directory.
    // Both should suppress independently.
    let mut wt = WhiteoutTracker::default();
    wt.insert_simple(Path::new("dir/file"), 2);
    wt.insert_opaque(Path::new("dir"), 3);

    // file suppressed by simple whiteout (layer 2)
    assert!(wt.is_suppressed(Path::new("dir/file"), 1));
    // other children suppressed by opaque (layer 3)
    assert!(wt.is_suppressed(Path::new("dir/other"), 2));
    // dir/other not suppressed from layer 3+ (same or newer than opaque)
    assert!(!wt.is_suppressed(Path::new("dir/other"), 3));
}

#[test]
fn whiteout_overwrite_simple_with_new_layer_index() {
    // If the same path is whited out again in a later layer, the later
    // whiteout should take effect (it overwrites the state).
    let mut wt = WhiteoutTracker::default();
    wt.insert_simple(Path::new("foo"), 1);

    // Layer 0 should be suppressed by whiteout from layer 1.
    assert!(wt.is_suppressed(Path::new("foo"), 0));

    // Now insert a newer whiteout at layer 3.
    wt.insert_simple(Path::new("foo"), 3);

    // Layer 2 should now be suppressed (was not before with layer_index=1).
    assert!(wt.is_suppressed(Path::new("foo"), 2));
    // Layer 3 (same as new whiteout) should not be suppressed.
    assert!(!wt.is_suppressed(Path::new("foo"), 3));
}

// ─── EmittedPathTracker ─────────────────────────────────────────────────────

#[test]
fn emitted_tracker_insert_and_contains() {
    let mut et = EmittedPathTracker::default();
    assert!(!et.contains(Path::new("foo")));
    et.insert(Path::new("foo"));
    assert!(et.contains(Path::new("foo")));
    assert!(!et.contains(Path::new("bar")));
}

#[test]
fn emitted_tracker_multiple_paths() {
    let mut et = EmittedPathTracker::default();
    et.insert(Path::new("a/b/c"));
    et.insert(Path::new("x/y"));
    assert!(et.contains(Path::new("a/b/c")));
    assert!(et.contains(Path::new("x/y")));
    assert!(!et.contains(Path::new("a/b")));
}

// ─── HardLinkTracker ────────────────────────────────────────────────────────

#[test]
fn hardlink_tracker_drain_sorted_orders_by_layer_index() {
    let ht = build_tracker_with_deferred(&[
        ("link3", "target3", 3),
        ("link1", "target1", 1),
        ("link2", "target2", 2),
    ]);

    let (deferred, _promotions) = ht.drain_sorted();
    let indices: Vec<usize> = deferred.iter().map(|e| e.layer_index).collect();
    assert_eq!(indices, vec![1, 2, 3], "deferred must be sorted by layer_index ascending");
}

#[test]
fn hardlink_tracker_promotions_sorted_by_layer_index() {
    let mut ht = HardLinkTracker::default();
    ht.record_promotion("alias3".into(), "target".into(), 3);
    ht.record_promotion("alias1".into(), "target".into(), 1);
    ht.record_promotion("alias2".into(), "target".into(), 2);

    let (_deferred, promotions) = ht.drain_sorted();
    let indices: Vec<usize> = promotions.iter().map(|e| e.layer_index).collect();
    assert_eq!(indices, vec![1, 2, 3], "promotions must be sorted by layer_index ascending");
}

#[test]
fn hardlink_tracker_note_suppressed_file_fulfils_pending_promotion() {
    let mut ht = HardLinkTracker::default();

    // Record a promotion first (cross-layer scenario: hardlink seen before target).
    ht.record_promotion("alias".into(), "target".into(), 1);

    // Now note the suppressed file for the target.
    let canonical = dummy_canonical();
    ht.note_suppressed_file("target".into(), canonical, b"content".to_vec());

    let (_deferred, promotions) = ht.drain_sorted();
    assert_eq!(promotions.len(), 1);
    let promo = &promotions[0];
    assert!(promo.file_data.is_some(), "promotion must have file_data after note_suppressed_file");
    let (_hdr, data) = promo.file_data.as_ref().unwrap();
    assert_eq!(data, b"content");
}

#[test]
fn hardlink_tracker_same_layer_suppressed_content_available_for_promotion() {
    let mut ht = HardLinkTracker::default();

    // Note suppressed file first (same-layer: file appears before hardlink in tar).
    let canonical = dummy_canonical();
    ht.note_suppressed_file("target".into(), canonical, b"data".to_vec());

    // Now record promotion — should pick up content immediately.
    ht.record_promotion("alias".into(), "target".into(), 0);

    let (_deferred, promotions) = ht.drain_sorted();
    assert_eq!(promotions.len(), 1);
    assert!(promotions[0].file_data.is_some(), "same-layer content must be attached immediately");
}

#[test]
fn hardlink_tracker_end_layer_clears_suppressed_content() {
    let mut ht = HardLinkTracker::default();

    let canonical = dummy_canonical();
    ht.note_suppressed_file("target".into(), canonical, b"data".to_vec());
    ht.end_layer();

    // After end_layer, new promotions should NOT get the old suppressed content.
    ht.record_promotion("alias".into(), "target".into(), 1);

    let (_deferred, promotions) = ht.drain_sorted();
    assert_eq!(promotions.len(), 1);
    assert!(
        promotions[0].file_data.is_none(),
        "suppressed content must be cleared after end_layer"
    );
}

#[test]
fn hardlink_tracker_multiple_promotions_same_target() {
    let mut ht = HardLinkTracker::default();

    let canonical = dummy_canonical();
    ht.note_suppressed_file("target".into(), canonical, b"shared".to_vec());

    ht.record_promotion("alias1".into(), "target".into(), 0);
    ht.record_promotion("alias2".into(), "target".into(), 0);

    let (_deferred, promotions) = ht.drain_sorted();
    assert_eq!(promotions.len(), 2);
    // Both should have file_data since they share the same target.
    assert!(promotions[0].file_data.is_some());
    assert!(promotions[1].file_data.is_some());
}

// ─── helpers ────────────────────────────────────────────────────────────────

fn dummy_canonical() -> oci2squashfs::canonical::CanonicalTarHeader {
    let mut hdr = tar::Header::new_ustar();
    hdr.set_entry_type(tar::EntryType::Regular);
    hdr.set_size(0);
    hdr.set_mode(0o644);
    hdr.set_mtime(0);
    hdr.set_uid(0);
    hdr.set_gid(0);
    hdr.set_path("placeholder").unwrap();
    hdr.set_cksum();
    oci2squashfs::canonical::CanonicalTarHeader {
        header: hdr,
        pax_extensions: vec![],
    }
}

fn build_tracker_with_deferred(entries: &[(&str, &str, usize)]) -> HardLinkTracker {
    let mut ht = HardLinkTracker::default();
    for (link, target, layer) in entries {
        let canonical = dummy_canonical();
        ht.record((*link).into(), (*target).into(), *layer, canonical);
    }
    ht
}
