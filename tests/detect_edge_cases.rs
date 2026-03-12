//! Edge case tests for detect_media_type and other image.rs functions.

use oci2squashfs::image::detect_media_type;

#[test]
fn detect_media_type_empty_file_returns_error() {
    // An empty file has no magic bytes — read_exact(4) should fail.
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), b"").unwrap();
    let result = detect_media_type(f.path());
    assert!(
        result.is_err(),
        "empty file must return an error (read_exact needs 4 bytes)"
    );
}

#[test]
fn detect_media_type_short_file_returns_error() {
    // A file with only 2 bytes cannot satisfy read_exact(4).
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), &[0x1f, 0x8b]).unwrap();
    let result = detect_media_type(f.path());
    assert!(
        result.is_err(),
        "file shorter than 4 bytes must return an error"
    );
}

#[test]
fn detect_media_type_exactly_4_bytes_succeeds() {
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), &[0x28, 0xb5, 0x2f, 0xfd]).unwrap();
    assert_eq!(
        detect_media_type(f.path()).unwrap(),
        "application/vnd.oci.image.layer.v1.tar+zstd"
    );
}

#[test]
fn detect_media_type_partial_gzip_magic_is_uncompressed() {
    // 0x1f followed by non-0x8b should not be detected as gzip.
    let f = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(f.path(), &[0x1f, 0x00, 0x00, 0x00]).unwrap();
    assert_eq!(
        detect_media_type(f.path()).unwrap(),
        "application/vnd.oci.image.layer.v1.tar",
        "partial gzip magic should fall through to uncompressed"
    );
}
