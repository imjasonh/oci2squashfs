//! Tests for normalize_path edge cases and additional merge edge cases
//! not covered by the main integration test suite.

#[path = "helpers/mod.rs"]
mod helpers;
use helpers::{
    LayerBuilder, blob, entry_type_in_tar, file_contents_in_tar, hardlink_target_in_tar, merge,
    paths_in_tar,
};

use oci2squashfs::overlay::normalize_path;
use std::path::Path;

// ─── normalize_path ─────────────────────────────────────────────────────────

#[test]
fn normalize_strips_dot_slash_prefix() {
    assert_eq!(normalize_path(Path::new("./foo")), Path::new("foo"));
}

#[test]
fn normalize_strips_leading_slash() {
    assert_eq!(normalize_path(Path::new("/foo")), Path::new("foo"));
}

#[test]
fn normalize_strips_dot_slash_then_slash() {
    // Path like ".//foo" — first trim "./" gives "/foo", then trim "/" gives "foo".
    assert_eq!(normalize_path(Path::new(".//foo")), Path::new("foo"));
}

#[test]
fn normalize_multiple_dot_slash_prefixes() {
    // "././foo" — trim_start_matches("./") strips all leading "./" runs.
    assert_eq!(normalize_path(Path::new("././foo")), Path::new("foo"));
}

#[test]
fn normalize_bare_dot_slash() {
    // "./" → "" (root entry, will be skipped by process_layer).
    assert_eq!(normalize_path(Path::new("./")), Path::new(""));
}

#[test]
fn normalize_bare_slash() {
    assert_eq!(normalize_path(Path::new("/")), Path::new(""));
}

#[test]
fn normalize_plain_path_unchanged() {
    assert_eq!(normalize_path(Path::new("usr/share/foo")), Path::new("usr/share/foo"));
}

#[test]
fn normalize_nested_path_with_dot_prefix() {
    assert_eq!(
        normalize_path(Path::new("./usr/share/foo")),
        Path::new("usr/share/foo")
    );
}

#[test]
fn normalize_multiple_leading_slashes() {
    // "///foo" — trim_start_matches('/') strips all leading slashes.
    assert_eq!(normalize_path(Path::new("///foo")), Path::new("foo"));
}

#[test]
fn normalize_dot_only() {
    assert_eq!(normalize_path(Path::new(".")), Path::new("."));
}

// ─── Additional merge edge cases ────────────────────────────────────────────

#[test]
fn test_single_layer_single_file() {
    // Simplest possible case: one layer, one file.
    let layer0 = LayerBuilder::new()
        .add_file("hello.txt", b"world", 0o644)
        .finish();
    let merged = merge(vec![blob(layer0, 0)]);
    let paths = paths_in_tar(&merged);
    assert_eq!(paths, vec!["hello.txt"]);
    assert_eq!(
        file_contents_in_tar(&merged, "hello.txt"),
        Some(b"world".to_vec())
    );
}

#[test]
fn test_empty_layer_produces_empty_tar() {
    // A layer with no entries should produce a valid (but empty) tar.
    let layer0 = LayerBuilder::new().finish();
    let merged = merge(vec![blob(layer0, 0)]);
    assert!(paths_in_tar(&merged).is_empty());
}

#[test]
fn test_whiteout_in_same_layer_as_target_suppresses_nothing() {
    // A whiteout in the same layer cannot suppress entries from that same layer.
    // This is because the whiteout's layer_index equals the entry's layer_index,
    // and suppression requires current_layer < layer_index.
    let layer0 = LayerBuilder::new()
        .add_file("foo.txt", b"data", 0o644)
        .add_whiteout("", "foo.txt")
        .finish();
    let merged = merge(vec![blob(layer0, 0)]);
    let paths = paths_in_tar(&merged);
    assert!(
        paths.iter().any(|p| p == "foo.txt"),
        "whiteout in same layer must not suppress same-layer entries"
    );
}

#[test]
fn test_three_layers_file_overwritten_twice() {
    // File overwritten in each successive layer — newest wins.
    let layer0 = LayerBuilder::new()
        .add_file("file.txt", b"v1", 0o644)
        .finish();
    let layer1 = LayerBuilder::new()
        .add_file("file.txt", b"v2", 0o644)
        .finish();
    let layer2 = LayerBuilder::new()
        .add_file("file.txt", b"v3", 0o644)
        .finish();
    let merged = merge(vec![blob(layer0, 0), blob(layer1, 1), blob(layer2, 2)]);

    let count = paths_in_tar(&merged)
        .iter()
        .filter(|p| p.as_str() == "file.txt")
        .count();
    assert_eq!(count, 1, "file must appear exactly once");
    assert_eq!(
        file_contents_in_tar(&merged, "file.txt"),
        Some(b"v3".to_vec()),
        "newest layer's content must win"
    );
}

#[test]
fn test_hardlink_to_file_in_same_layer() {
    // Hardlink and target in the same layer — the link should resolve normally.
    let layer0 = LayerBuilder::new()
        .add_file("target.txt", b"data", 0o644)
        .add_hardlink("link.txt", "target.txt")
        .finish();
    let merged = merge(vec![blob(layer0, 0)]);
    let paths = paths_in_tar(&merged);
    assert!(paths.iter().any(|p| p == "target.txt"));
    assert!(paths.iter().any(|p| p == "link.txt"));
    assert_eq!(
        hardlink_target_in_tar(&merged, "link.txt").as_deref(),
        Some("target.txt")
    );
}

#[test]
fn test_hardlink_target_replaced_newer_layer_link_resolves_to_new() {
    // Layer 0: target.txt ("old")
    // Layer 1: target.txt ("new"), link.txt -> target.txt
    // Both in newer layer, link should resolve to new target.
    let layer0 = LayerBuilder::new()
        .add_file("target.txt", b"old", 0o644)
        .finish();
    let layer1 = LayerBuilder::new()
        .add_file("target.txt", b"new", 0o644)
        .add_hardlink("link.txt", "target.txt")
        .finish();
    let merged = merge(vec![blob(layer0, 0), blob(layer1, 1)]);
    let paths = paths_in_tar(&merged);
    assert!(paths.iter().any(|p| p == "target.txt"));
    assert!(paths.iter().any(|p| p == "link.txt"));
    assert_eq!(
        file_contents_in_tar(&merged, "target.txt"),
        Some(b"new".to_vec())
    );
}

#[test]
fn test_deeply_nested_path() {
    // Ensure deeply nested paths work correctly.
    let deep_path = "a/b/c/d/e/f/g/h/i/j/file.txt";
    let layer0 = LayerBuilder::new()
        .add_dir("a")
        .add_dir("a/b")
        .add_dir("a/b/c")
        .add_dir("a/b/c/d")
        .add_dir("a/b/c/d/e")
        .add_dir("a/b/c/d/e/f")
        .add_dir("a/b/c/d/e/f/g")
        .add_dir("a/b/c/d/e/f/g/h")
        .add_dir("a/b/c/d/e/f/g/h/i")
        .add_dir("a/b/c/d/e/f/g/h/i/j")
        .add_file(deep_path, b"deep", 0o644)
        .finish();
    let merged = merge(vec![blob(layer0, 0)]);
    assert!(paths_in_tar(&merged).iter().any(|p| p == deep_path));
    assert_eq!(
        file_contents_in_tar(&merged, deep_path),
        Some(b"deep".to_vec())
    );
}

#[test]
fn test_opaque_whiteout_on_subdirectory_suppresses_all_older_children() {
    // Opaque whiteout on a subdirectory should suppress everything beneath
    // it from older layers, but not siblings.
    let layer0 = LayerBuilder::new()
        .add_dir("dir")
        .add_file("dir/old.txt", b"old", 0o644)
        .add_file("dir/old2.txt", b"old2", 0o644)
        .add_file("sibling.txt", b"sibling", 0o644)
        .finish();
    let layer1 = LayerBuilder::new()
        .add_dir("dir")
        .add_opaque_whiteout("dir")
        .add_file("dir/new.txt", b"new", 0o644)
        .finish();
    let merged = merge(vec![blob(layer0, 0), blob(layer1, 1)]);
    let paths = paths_in_tar(&merged);
    assert!(
        paths.iter().any(|p| p == "dir/new.txt"),
        "new.txt from newer layer must survive"
    );
    assert!(
        !paths.iter().any(|p| p == "dir/old.txt"),
        "old.txt from older layer must be suppressed by opaque whiteout"
    );
    assert!(
        !paths.iter().any(|p| p == "dir/old2.txt"),
        "old2.txt from older layer must be suppressed by opaque whiteout"
    );
    assert!(
        paths.iter().any(|p| p == "sibling.txt"),
        "sibling.txt outside the opaque dir must survive"
    );
}

#[test]
fn test_symlink_preserved_across_layers() {
    // Symlinks in older layers should survive if not whited out.
    let layer0 = LayerBuilder::new()
        .add_symlink("link", "/usr/bin/target")
        .finish();
    let layer1 = LayerBuilder::new()
        .add_file("unrelated.txt", b"data", 0o644)
        .finish();
    let merged = merge(vec![blob(layer0, 0), blob(layer1, 1)]);
    let paths = paths_in_tar(&merged);
    assert!(paths.iter().any(|p| p == "link"));
    assert!(paths.iter().any(|p| p == "unrelated.txt"));
}

#[test]
fn test_many_layers_ordering() {
    // Test with 5 layers to ensure ordering is correct beyond 2-3 layers.
    let layer0 = LayerBuilder::new()
        .add_file("f0.txt", b"v0", 0o644)
        .finish();
    let layer1 = LayerBuilder::new()
        .add_file("f1.txt", b"v1", 0o644)
        .finish();
    let layer2 = LayerBuilder::new()
        .add_file("f2.txt", b"v2", 0o644)
        .finish();
    let layer3 = LayerBuilder::new()
        .add_file("f3.txt", b"v3", 0o644)
        .finish();
    let layer4 = LayerBuilder::new()
        .add_file("f4.txt", b"v4", 0o644)
        .finish();
    let merged = merge(vec![
        blob(layer0, 0),
        blob(layer1, 1),
        blob(layer2, 2),
        blob(layer3, 3),
        blob(layer4, 4),
    ]);
    let paths = paths_in_tar(&merged);
    for i in 0..5 {
        let name = format!("f{i}.txt");
        assert!(paths.iter().any(|p| p == &name), "f{i}.txt must be present");
    }
}

#[test]
fn test_large_file_content_preserved() {
    // Test with a file larger than typical buffer sizes.
    let large_content: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
    let layer0 = LayerBuilder::new()
        .add_file("large.bin", &large_content, 0o644)
        .finish();
    let merged = merge(vec![blob(layer0, 0)]);
    assert_eq!(
        file_contents_in_tar(&merged, "large.bin"),
        Some(large_content),
        "large file content must be preserved exactly"
    );
}

#[test]
fn test_whiteout_then_recreate_with_different_type() {
    // Layer 0: file at "path"
    // Layer 1: whiteout "path", then recreate as symlink
    // Expected: "path" appears as a symlink, not a file.
    let layer0 = LayerBuilder::new()
        .add_file("path", b"data", 0o644)
        .finish();
    let layer1 = LayerBuilder::new()
        .add_whiteout("", "path")
        .add_symlink("path", "elsewhere")
        .finish();
    let merged = merge(vec![blob(layer0, 0), blob(layer1, 1)]);
    assert_eq!(
        entry_type_in_tar(&merged, "path"),
        Some(tar::EntryType::Symlink),
        "recreated entry must be a symlink"
    );
}

#[test]
fn test_deferred_hardlink_target_not_emitted_is_dropped() {
    // A hardlink whose target was never emitted (e.g., target doesn't exist
    // in any layer) should be silently dropped.
    let layer0 = LayerBuilder::new()
        .add_hardlink("orphan_link.txt", "nonexistent_target.txt")
        .finish();
    let merged = merge(vec![blob(layer0, 0)]);
    let paths = paths_in_tar(&merged);
    assert!(
        !paths.iter().any(|p| p == "orphan_link.txt"),
        "hardlink to non-existent target must be dropped"
    );
}

#[test]
fn test_directory_entry_deduplication_across_layers() {
    // Both layers emit the same directory. Only one should appear in output.
    let layer0 = LayerBuilder::new()
        .add_dir("shared_dir")
        .add_file("shared_dir/old.txt", b"old", 0o644)
        .finish();
    let layer1 = LayerBuilder::new()
        .add_dir("shared_dir")
        .add_file("shared_dir/new.txt", b"new", 0o644)
        .finish();
    let merged = merge(vec![blob(layer0, 0), blob(layer1, 1)]);
    let paths = paths_in_tar(&merged);

    let dir_count = paths.iter().filter(|p| p.as_str() == "shared_dir").count();
    assert_eq!(dir_count, 1, "directory entry must appear exactly once");
    // Both files should be present (no whiteout).
    assert!(paths.iter().any(|p| p == "shared_dir/old.txt"));
    assert!(paths.iter().any(|p| p == "shared_dir/new.txt"));
}
