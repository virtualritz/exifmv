//! Comprehensive tests for exifmv.
//!
//! All tests use `tempfile::TempDir` which creates directories in the system
//! temp directory, ensuring no artifacts are left in the source tree.

use crate::{Template, TemplateContext, day_wrap, move_image, util::move_file};
use chrono::NaiveTime;
use clap::{Arg, ArgAction, ArgMatches, Command};
use exif::DateTime;
use indicatif::MultiProgress;
use std::{fs, io::Write, path::Path, sync::Arc};
use tempfile::TempDir;

/// Creates a minimal valid JPEG file with EXIF DateTimeOriginal tag.
///
/// The datetime format is "YYYY:MM:DD HH:MM:SS".
fn create_test_jpeg(path: &Path, datetime: &str) {
    // JPEG with EXIF structure:
    // - SOI (Start of Image)
    // - APP1 (EXIF segment)
    // - Minimal image data
    // - EOI (End of Image)

    let datetime_bytes = datetime.as_bytes();
    assert_eq!(datetime_bytes.len(), 19, "DateTime must be 19 bytes");

    // Build EXIF APP1 segment.
    // TIFF header starts after "Exif\0\0".
    // We use little-endian (II) format.
    let mut exif_data: Vec<u8> = Vec::new();

    // Exif identifier.
    exif_data.extend_from_slice(b"Exif\x00\x00");

    // TIFF Header (8 bytes).
    // Byte order: little-endian (II).
    exif_data.extend_from_slice(b"II");
    // TIFF magic (42).
    exif_data.extend_from_slice(&42u16.to_le_bytes());
    // Offset to IFD0 (8 bytes from TIFF header start).
    exif_data.extend_from_slice(&8u32.to_le_bytes());

    // IFD0 (Image File Directory 0).
    // We have 1 entry pointing to EXIF IFD.
    let ifd0_offset = 8u32;
    let ifd0_entry_count = 1u16;
    exif_data.extend_from_slice(&ifd0_entry_count.to_le_bytes());

    // IFD0 Entry: ExifIFDPointer (tag 0x8769).
    // Points to EXIF IFD.
    let exif_ifd_offset = ifd0_offset + 2 + 12 + 4; // After IFD0.
    exif_data.extend_from_slice(&0x8769u16.to_le_bytes()); // Tag.
    exif_data.extend_from_slice(&4u16.to_le_bytes()); // Type: LONG.
    exif_data.extend_from_slice(&1u32.to_le_bytes()); // Count.
    exif_data.extend_from_slice(&exif_ifd_offset.to_le_bytes()); // Value (offset).

    // Next IFD offset (0 = none).
    exif_data.extend_from_slice(&0u32.to_le_bytes());

    // EXIF IFD.
    let exif_ifd_entry_count = 1u16;
    exif_data.extend_from_slice(&exif_ifd_entry_count.to_le_bytes());

    // EXIF IFD Entry: DateTimeOriginal (tag 0x9003).
    let datetime_offset = exif_ifd_offset + 2 + 12 + 4; // After EXIF IFD.
    exif_data.extend_from_slice(&0x9003u16.to_le_bytes()); // Tag.
    exif_data.extend_from_slice(&2u16.to_le_bytes()); // Type: ASCII.
    exif_data.extend_from_slice(&20u32.to_le_bytes()); // Count (19 + null).
    exif_data.extend_from_slice(&datetime_offset.to_le_bytes()); // Value (offset).

    // Next IFD offset (0 = none).
    exif_data.extend_from_slice(&0u32.to_le_bytes());

    // DateTimeOriginal value (20 bytes with null terminator).
    exif_data.extend_from_slice(datetime_bytes);
    exif_data.push(0);

    // APP1 segment length (includes length bytes but not marker).
    let app1_length = (exif_data.len() + 2) as u16;

    // Build complete JPEG.
    let mut jpeg: Vec<u8> = Vec::new();

    // SOI.
    jpeg.extend_from_slice(&[0xFF, 0xD8]);

    // APP1 (EXIF).
    jpeg.extend_from_slice(&[0xFF, 0xE1]);
    jpeg.extend_from_slice(&app1_length.to_be_bytes());
    jpeg.extend_from_slice(&exif_data);

    // Minimal valid JPEG image data.
    // DQT (Define Quantization Table).
    jpeg.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x43, 0x00]);
    jpeg.extend_from_slice(&[16u8; 64]); // Quantization values.

    // SOF0 (Start of Frame, baseline DCT).
    // 1x1 pixel, 1 component.
    jpeg.extend_from_slice(&[
        0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11,
        0x00,
    ]);

    // DHT (Define Huffman Table) - DC table.
    jpeg.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x00]);
    jpeg.extend_from_slice(&[0u8; 16]); // Code counts.
    jpeg.extend_from_slice(&[0u8; 12]); // Values.

    // DHT - AC table.
    jpeg.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x10]);
    jpeg.extend_from_slice(&[0u8; 16]);
    jpeg.extend_from_slice(&[0u8; 12]);

    // SOS (Start of Scan).
    jpeg.extend_from_slice(&[
        0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00,
    ]);

    // Minimal scan data.
    jpeg.push(0x00);

    // EOI.
    jpeg.extend_from_slice(&[0xFF, 0xD9]);

    // Write to file.
    let mut file = fs::File::create(path).expect("Failed to create test JPEG");
    file.write_all(&jpeg).expect("Failed to write test JPEG");
}

/// Creates a JPEG without EXIF data.
fn create_jpeg_without_exif(path: &Path) {
    let mut jpeg: Vec<u8> = Vec::new();

    // SOI.
    jpeg.extend_from_slice(&[0xFF, 0xD8]);

    // DQT.
    jpeg.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x43, 0x00]);
    jpeg.extend_from_slice(&[16u8; 64]);

    // SOF0.
    jpeg.extend_from_slice(&[
        0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11,
        0x00,
    ]);

    // DHT - DC.
    jpeg.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x00]);
    jpeg.extend_from_slice(&[0u8; 16]);
    jpeg.extend_from_slice(&[0u8; 12]);

    // DHT - AC.
    jpeg.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x10]);
    jpeg.extend_from_slice(&[0u8; 16]);
    jpeg.extend_from_slice(&[0u8; 12]);

    // SOS.
    jpeg.extend_from_slice(&[
        0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00,
    ]);
    jpeg.push(0x00);

    // EOI.
    jpeg.extend_from_slice(&[0xFF, 0xD9]);

    let mut file = fs::File::create(path).expect("Failed to create test JPEG");
    file.write_all(&jpeg).expect("Failed to write test JPEG");
}

/// Builds test `ArgMatches` with specified flags.
fn make_test_args(flags: &[&str]) -> Arc<ArgMatches> {
    let cmd = Command::new("test")
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("remove-source")
                .long("remove-source")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("trash-source")
                .long("trash-source")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("make-lowercase")
                .long("make-lowercase")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("checksum")
                .long("checksum")
                .action(ArgAction::SetTrue),
        );

    let args: Vec<_> = std::iter::once(&"test").chain(flags.iter()).collect();

    Arc::new(cmd.get_matches_from(args))
}

// =============================================================================
// move_file() Tests - Critical Data Loss Scenarios
// =============================================================================

#[test]
fn move_to_new_location() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    fs::write(&source, b"test content").unwrap();
    let args = make_test_args(&[]);

    move_file(&source, &dest, false, args, &MultiProgress::new()).unwrap();

    assert!(!source.exists(), "Source should be moved");
    assert!(dest.exists(), "Destination should exist");
    assert_eq!(fs::read(&dest).unwrap(), b"test content");
}

#[test]
fn skip_when_source_equals_dest() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("file.jpg");

    fs::write(&file, b"test content").unwrap();
    let args = make_test_args(&[]);

    // Should not error when source == dest.
    move_file(&file, &file, false, args, &MultiProgress::new()).unwrap();

    assert!(file.exists(), "File should still exist");
    assert_eq!(fs::read(&file).unwrap(), b"test content");
}

#[test]
fn skip_existing_same_size_no_flags() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    // Same size, different content.
    fs::write(&source, b"content A").unwrap();
    fs::write(&dest, b"content B").unwrap();

    let args = make_test_args(&[]);
    move_file(&source, &dest, false, args, &MultiProgress::new()).unwrap();

    // Both files should be preserved (default behavior).
    assert!(source.exists(), "Source should be preserved");
    assert!(dest.exists(), "Destination should be preserved");
    assert_eq!(fs::read(&source).unwrap(), b"content A");
    assert_eq!(fs::read(&dest).unwrap(), b"content B");
}

#[test]
fn remove_source_deletes_on_duplicate() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    // Same size files (duplicate detected by size).
    fs::write(&source, b"same size").unwrap();
    fs::write(&dest, b"same size").unwrap();

    let args = make_test_args(&["--remove-source"]);
    move_file(&source, &dest, false, args, &MultiProgress::new()).unwrap();

    // Source should be deleted, dest preserved.
    assert!(!source.exists(), "Source should be deleted");
    assert!(dest.exists(), "Destination should be preserved");
}

#[test]
fn remove_source_preserves_different_size() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    // Different sizes.
    fs::write(&source, b"short").unwrap();
    fs::write(&dest, b"longer content").unwrap();

    let args = make_test_args(&["--remove-source"]);
    move_file(&source, &dest, false, args, &MultiProgress::new()).unwrap();

    // Both should be preserved when sizes differ.
    assert!(
        source.exists(),
        "Source should be preserved (size mismatch)"
    );
    assert!(dest.exists(), "Destination should be preserved");
}

#[test]
fn dry_run_no_file_changes() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    fs::write(&source, b"original").unwrap();

    let args = make_test_args(&["--dry-run"]);
    move_file(&source, &dest, false, args, &MultiProgress::new()).unwrap();

    // Dry run should not move files.
    assert!(source.exists(), "Source should exist (dry run)");
    assert!(
        !dest.exists(),
        "Destination should not be created (dry run)"
    );
}

#[test]
fn dry_run_preserves_source_on_duplicate() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    fs::write(&source, b"same size").unwrap();
    fs::write(&dest, b"same size").unwrap();

    // Even with --remove-source, dry-run should preserve.
    let args = make_test_args(&["--dry-run", "--remove-source"]);
    move_file(&source, &dest, false, args, &MultiProgress::new()).unwrap();

    assert!(source.exists(), "Source should exist (dry run)");
    assert!(dest.exists(), "Destination should exist");
}

#[test]
fn different_size_preserves_both() {
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    fs::write(&source, b"A").unwrap();
    fs::write(&dest, b"BB").unwrap();

    let args = make_test_args(&[]);
    move_file(&source, &dest, false, args, &MultiProgress::new()).unwrap();

    assert!(source.exists(), "Source preserved (size mismatch)");
    assert!(dest.exists(), "Dest preserved");
    assert_eq!(fs::read(&source).unwrap(), b"A");
    assert_eq!(fs::read(&dest).unwrap(), b"BB");
}

// =============================================================================
// move_image() Integration Tests
// =============================================================================

#[test]
fn move_image_creates_date_hierarchy() {
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let dest_dir = tmp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let source_file = source_dir.join("IMG_1234.jpg");
    create_test_jpeg(&source_file, "2023:08:15 14:30:00");

    let template =
        Template::parse("{year}/{month}/{day}/{filename}.{extension}").unwrap();
    let time_offset = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let args = make_test_args(&[]);

    move_image(
        &source_file,
        &dest_dir,
        &time_offset,
        &template,
        false,
        false,
        args,
        Arc::new(MultiProgress::new()),
    )
    .unwrap();

    let expected = dest_dir.join("2023/08/15/IMG_1234.jpg");
    assert!(
        expected.exists(),
        "File should be at {}",
        expected.display()
    );
    assert!(!source_file.exists(), "Source should be moved");
}

#[test]
fn move_image_missing_exif_fails() {
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let dest_dir = tmp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let source_file = source_dir.join("no_exif.jpg");
    create_jpeg_without_exif(&source_file);

    let template =
        Template::parse("{year}/{month}/{day}/{filename}.{extension}").unwrap();
    let time_offset = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let args = make_test_args(&[]);

    let result = move_image(
        &source_file,
        &dest_dir,
        &time_offset,
        &template,
        false,
        false,
        args,
        Arc::new(MultiProgress::new()),
    );

    assert!(result.is_err(), "Should fail without EXIF");
    assert!(source_file.exists(), "Source should be preserved on error");
}

#[test]
fn move_image_respects_custom_template() {
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let dest_dir = tmp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let source_file = source_dir.join("photo.jpg");
    create_test_jpeg(&source_file, "2024:12:25 10:00:00");

    let template =
        Template::parse("{year}-{month}-{day}_{filename}.{extension}").unwrap();
    let time_offset = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let args = make_test_args(&[]);

    move_image(
        &source_file,
        &dest_dir,
        &time_offset,
        &template,
        false,
        false,
        args,
        Arc::new(MultiProgress::new()),
    )
    .unwrap();

    let expected = dest_dir.join("2024-12-25_photo.jpg");
    assert!(
        expected.exists(),
        "File should be at {}",
        expected.display()
    );
}

#[test]
fn move_image_lowercase_option() {
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let dest_dir = tmp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let source_file = source_dir.join("IMG_UPPER.JPG");
    create_test_jpeg(&source_file, "2023:01:01 12:00:00");

    let template = Template::parse("{filename}.{extension}").unwrap();
    let time_offset = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let args = make_test_args(&[]);

    move_image(
        &source_file,
        &dest_dir,
        &time_offset,
        &template,
        true,
        false,
        args,
        Arc::new(MultiProgress::new()),
    )
    .unwrap();

    let expected = dest_dir.join("img_upper.jpg");
    assert!(expected.exists(), "Filename should be lowercase");
}

#[test]
fn move_image_day_wrap_shifts_date() {
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let dest_dir = tmp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    // Photo taken at 23:30 with 1-hour day wrap.
    let source_file = source_dir.join("late_night.jpg");
    create_test_jpeg(&source_file, "2023:08:21 23:30:00");

    let template =
        Template::parse("{year}/{month}/{day}/{filename}.{extension}").unwrap();
    // Day wraps at 01:00, so 23:30 + 01:00 > 24:00 means next day.
    let time_offset = NaiveTime::from_hms_opt(1, 0, 0).unwrap();
    let args = make_test_args(&[]);

    move_image(
        &source_file,
        &dest_dir,
        &time_offset,
        &template,
        false,
        false,
        args,
        Arc::new(MultiProgress::new()),
    )
    .unwrap();

    // Should be in 08/22 due to day wrap.
    let expected = dest_dir.join("2023/08/22/late_night.jpg");
    assert!(expected.exists(), "Date should be shifted by day wrap");
}

// =============================================================================
// XMP Sidecar Tests
// =============================================================================

#[test]
fn xmp_sidecar_moves_with_image() {
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let dest_dir = tmp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let source_file = source_dir.join("photo.jpg");
    let source_xmp = source_dir.join("photo.jpg.xmp");
    create_test_jpeg(&source_file, "2023:06:15 09:00:00");
    fs::write(&source_xmp, b"<xmp>metadata</xmp>").unwrap();

    let template =
        Template::parse("{year}/{month}/{day}/{filename}.{extension}").unwrap();
    let time_offset = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let args = make_test_args(&[]);

    move_image(
        &source_file,
        &dest_dir,
        &time_offset,
        &template,
        false,
        false,
        args,
        Arc::new(MultiProgress::new()),
    )
    .unwrap();

    let expected_jpg = dest_dir.join("2023/06/15/photo.jpg");
    let expected_xmp = dest_dir.join("2023/06/15/photo.jpg.xmp");

    assert!(expected_jpg.exists(), "Image should be moved");
    assert!(expected_xmp.exists(), "XMP sidecar should follow image");
    assert!(!source_xmp.exists(), "Source XMP should be moved");
}

#[test]
fn xmp_uppercase_moves_with_image() {
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let dest_dir = tmp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let source_file = source_dir.join("photo.jpg");
    let source_xmp = source_dir.join("photo.jpg.XMP");
    create_test_jpeg(&source_file, "2023:06:15 09:00:00");
    fs::write(&source_xmp, b"<xmp>metadata</xmp>").unwrap();

    let template =
        Template::parse("{year}/{month}/{day}/{filename}.{extension}").unwrap();
    let time_offset = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let args = make_test_args(&[]);

    move_image(
        &source_file,
        &dest_dir,
        &time_offset,
        &template,
        false,
        false,
        args,
        Arc::new(MultiProgress::new()),
    )
    .unwrap();

    let expected_xmp = dest_dir.join("2023/06/15/photo.jpg.XMP");
    assert!(expected_xmp.exists(), "Uppercase XMP should be preserved");
}

#[test]
fn xmp_lowercase_conversion() {
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let dest_dir = tmp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let source_file = source_dir.join("PHOTO.JPG");
    let source_xmp = source_dir.join("PHOTO.JPG.XMP");
    create_test_jpeg(&source_file, "2023:06:15 09:00:00");
    fs::write(&source_xmp, b"<xmp>metadata</xmp>").unwrap();

    let template = Template::parse("{filename}.{extension}").unwrap();
    let time_offset = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let args = make_test_args(&[]);

    move_image(
        &source_file,
        &dest_dir,
        &time_offset,
        &template,
        true,
        false,
        args,
        Arc::new(MultiProgress::new()),
    )
    .unwrap();

    let expected_xmp = dest_dir.join("photo.jpg.xmp");
    assert!(
        expected_xmp.exists(),
        "XMP extension should be lowercase with --make-lowercase"
    );
}

// =============================================================================
// day_wrap() Unit Tests
// =============================================================================

#[test]
fn day_wrap_no_wrap_early_time() {
    let ts = DateTime {
        year: 2023,
        month: 8,
        day: 15,
        hour: 10,
        minute: 30,
        second: 0,
        nanosecond: None,
        offset: None,
    };
    let offset = NaiveTime::from_hms_opt(4, 0, 0).unwrap();
    assert_eq!(day_wrap(&ts, &offset), 0);
}

#[test]
fn day_wrap_wraps_late_night() {
    let ts = DateTime {
        year: 2023,
        month: 8,
        day: 15,
        hour: 22,
        minute: 0,
        second: 0,
        nanosecond: None,
        offset: None,
    };
    let offset = NaiveTime::from_hms_opt(4, 0, 0).unwrap();
    // 22 + 4 = 26 > 23, so wraps.
    assert_eq!(day_wrap(&ts, &offset), 1);
}

#[test]
fn day_wrap_minute_overflow_causes_wrap() {
    let ts = DateTime {
        year: 2023,
        month: 8,
        day: 15,
        hour: 23,
        minute: 30,
        second: 0,
        nanosecond: None,
        offset: None,
    };
    let offset = NaiveTime::from_hms_opt(0, 31, 0).unwrap();
    // 23:30 + 0:31 = 24:01, minute overflow adds 1 to hour check.
    assert_eq!(day_wrap(&ts, &offset), 1);
}

// =============================================================================
// Config Tests
// =============================================================================

#[test]
fn config_load_missing_returns_default() {
    use crate::config::Config;

    let tmp = TempDir::new().unwrap();
    let missing_path = tmp.path().join("nonexistent.toml");

    // Loading from a non-existent path should return default (confy creates
    // it).
    let config = Config::load(Some(&missing_path)).unwrap();
    assert!(config.format.is_none());
    assert!(config.verbose.is_none());
}

#[test]
fn config_load_custom_path() {
    use crate::config::Config;

    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("custom.toml");

    fs::write(
        &config_path,
        r#"
format = "{year}-{month}/{filename}.{extension}"
verbose = true
make-lowercase = true
"#,
    )
    .unwrap();

    let config = Config::load(Some(&config_path)).unwrap();
    assert_eq!(
        config.format.as_deref(),
        Some("{year}-{month}/{filename}.{extension}")
    );
    assert_eq!(config.verbose, Some(true));
    assert_eq!(config.make_lowercase, Some(true));
}

#[test]
fn config_format_returns_default_when_unset() {
    use crate::config::{Config, DEFAULT_FORMAT};

    let config = Config::default();
    assert_eq!(config.format(), DEFAULT_FORMAT);
}

// =============================================================================
// Template Tests (additional coverage)
// =============================================================================

#[test]
fn template_expand_with_all_fields() {
    let template = Template::parse(
        "{year}/{month}/{day}/{hour}{minute}{second}_{filename}.{extension}",
    )
    .unwrap();

    let ctx = TemplateContext {
        year: "2023".to_string(),
        month: "08".to_string(),
        day: "15".to_string(),
        hour: "14".to_string(),
        minute: "30".to_string(),
        second: "45".to_string(),
        filename: "IMG_001".to_string(),
        extension: "jpg".to_string(),
        camera_make: Some("Canon".to_string()),
        camera_model: Some("EOS R5".to_string()),
        lens: None,
        iso: None,
        focal_length: None,
    };

    let result = template.expand(&ctx);
    assert_eq!(result, "2023/08/15/143045_IMG_001.jpg");
}

#[test]
fn template_expand_optional_fields_fallback() {
    let template =
        Template::parse("{camera_make}/{camera_model}/{lens}").unwrap();

    let ctx = TemplateContext {
        year: "2023".to_string(),
        month: "08".to_string(),
        day: "15".to_string(),
        hour: "12".to_string(),
        minute: "00".to_string(),
        second: "00".to_string(),
        filename: "test".to_string(),
        extension: "jpg".to_string(),
        camera_make: None,
        camera_model: None,
        lens: None,
        iso: None,
        focal_length: None,
    };

    let result = template.expand(&ctx);
    assert_eq!(result, "unknown/unknown/unknown");
}

// =============================================================================
// Edge Case: Duplicate Detection
// =============================================================================

#[test]
fn same_size_different_content_risk() {
    // This test documents the current behavior: size-based duplicate detection.
    // Two files with same size but different content are treated as duplicates.
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    // Same size (9 bytes), different content.
    fs::write(&source, b"AAAAAAAAA").unwrap();
    fs::write(&dest, b"BBBBBBBBB").unwrap();

    let args = make_test_args(&["--remove-source"]);
    move_file(&source, &dest, false, args, &MultiProgress::new()).unwrap();

    // Current behavior: source is deleted because sizes match.
    // This is a known limitation - size-based detection can have false
    // positives.
    assert!(!source.exists(), "Source deleted (size match)");
    assert_eq!(fs::read(&dest).unwrap(), b"BBBBBBBBB", "Dest unchanged");
}

// =============================================================================
// Checksum-Based Duplicate Detection
// =============================================================================

#[test]
fn checksum_detects_different_content_same_size() {
    // With --checksum, same-size different-content files are NOT treated as
    // duplicates.
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    // Same size (9 bytes), different content.
    fs::write(&source, b"AAAAAAAAA").unwrap();
    fs::write(&dest, b"BBBBBBBBB").unwrap();

    let args = make_test_args(&["--remove-source", "--checksum"]);
    move_file(&source, &dest, true, args, &MultiProgress::new()).unwrap();

    // With checksum: source is preserved because content differs.
    assert!(source.exists(), "Source preserved (checksum differs)");
    assert!(dest.exists(), "Dest preserved");
    assert_eq!(fs::read(&source).unwrap(), b"AAAAAAAAA");
    assert_eq!(fs::read(&dest).unwrap(), b"BBBBBBBBB");
}

#[test]
fn checksum_removes_true_duplicates() {
    // With --checksum --remove-source, identical files result in source
    // deletion.
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    // Identical content.
    fs::write(&source, b"IDENTICAL").unwrap();
    fs::write(&dest, b"IDENTICAL").unwrap();

    let args = make_test_args(&["--remove-source", "--checksum"]);
    move_file(&source, &dest, true, args, &MultiProgress::new()).unwrap();

    // Source is removed because checksums match.
    assert!(!source.exists(), "Source removed (true duplicate)");
    assert!(dest.exists(), "Dest preserved");
    assert_eq!(fs::read(&dest).unwrap(), b"IDENTICAL");
}

#[test]
fn checksum_skips_without_remove_source() {
    // With --checksum but without --remove-source, duplicates are skipped but
    // preserved.
    let tmp = TempDir::new().unwrap();
    let source = tmp.path().join("source.jpg");
    let dest = tmp.path().join("dest.jpg");

    fs::write(&source, b"SAME CONTENT").unwrap();
    fs::write(&dest, b"SAME CONTENT").unwrap();

    let args = make_test_args(&["--checksum", "--verbose"]);
    move_file(&source, &dest, true, args, &MultiProgress::new()).unwrap();

    // Both preserved - just skipped.
    assert!(source.exists(), "Source preserved");
    assert!(dest.exists(), "Dest preserved");
}
