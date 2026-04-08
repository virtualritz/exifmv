use crate::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::info;
use std::{
    fs,
    io::{self, BufReader, ErrorKind, Read},
};
use xxhash_rust::xxh3::{Xxh3, xxh3_64};

const EXTENSIONS: &[&str] = &[
    // RAW file extensions.
    "3fr", "ari", "arw", "bay", "cap", "cr2", "cr3", "crw", "data", "dcr",
    "dcs", "dng", "drf", "eip", "erf", "fff", "gpr", "iiq", "k25", "kdc",
    "mdc", "mef", "mos", "mrw", "nef", "nrw", "obm", "orf", "pef", "ptx",
    "pxn", "r3d", "raf", "raw", "rw2", "rwl", "rwz", "sr2", "srf", "srw",
    "x3f", // Other image files.
    "avif", "bmp", "fpx", "gif", "heic", "heif", "j2k", "jfif", "jif", "jp2",
    "jpeg", "jpg", "jpx", "pcd", "png", "psd", "tif", "tiff",
    "webp", // Movie file formats.
    "264", "3g2", "3gp", "amv", "asf", "avi", "cine", "drc", "f4a", "f4b",
    "f4p", "f4v", "flv", "gifv", "m2ts", "m2v", "m4p", "m4v", "mkv", "mng",
    "mp4", "mpeg", "mpg", "mts", "mxf", "nsv", "ogg", "qt", "roq", "svi",
    "vob", "wmv", "yuv",
];

pub(crate) fn has_image_extension(entry: &walkdir::DirEntry) -> bool {
    if let Some(extension) = PathBuf::from(entry.file_name()).extension()
        && let Some(extension) = extension.to_str()
    {
        EXTENSIONS.contains(&extension.to_lowercase().as_str())
    } else {
        false
    }
}

/// Files larger than 64MB use streaming hash to avoid memory pressure.
const STREAMING_THRESHOLD: u64 = 64 * 1024 * 1024;
/// Buffer size for streaming hash (64KB).
const HASH_BUFFER_SIZE: usize = 64 * 1024;

/// Compute XXH3-64 hash of a file.
/// Uses streaming for files larger than `STREAMING_THRESHOLD` to reduce memory
/// usage.
fn file_hash(path: &Path, size: u64) -> Result<u64> {
    let mut file = fs::File::open(path).with_context(|| {
        format!("Unable to open '{}' for hashing.", path.display())
    })?;

    if size <= STREAMING_THRESHOLD {
        // Small files: read entire file into memory (fast path).
        let mut buffer = Vec::with_capacity(size as usize);
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Unable to read '{}' for hashing.", path.display())
        })?;
        Ok(xxh3_64(&buffer))
    } else {
        // Large files: stream with fixed buffer.
        let mut hasher = Xxh3::new();
        let mut buffer = [0u8; HASH_BUFFER_SIZE];
        loop {
            let bytes_read = file.read(&mut buffer).with_context(|| {
                format!("Unable to read '{}' for hashing.", path.display())
            })?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        Ok(hasher.digest())
    }
}

/// Check if two files are duplicates.
/// If `use_checksum` is true, compares file contents via XXH3 hash.
/// Otherwise, only compares file sizes.
fn files_match(
    source: &Path,
    dest: &Path,
    source_size: u64,
    dest_size: u64,
    use_checksum: bool,
) -> Result<bool> {
    if source_size != dest_size {
        return Ok(false);
    }
    if use_checksum {
        let source_hash = file_hash(source, source_size)?;
        let dest_hash = file_hash(dest, dest_size)?;
        Ok(source_hash == dest_hash)
    } else {
        Ok(true)
    }
}

/// Move a file, falling back to copy+delete with a progress bar for
/// cross-device moves.
fn move_or_copy(
    source: &Path,
    dest: &Path,
    multi: &MultiProgress,
) -> io::Result<()> {
    match fs::rename(source, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            let size = source.metadata()?.len();
            let pb = multi.add(ProgressBar::new(size));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{msg} [{bar:30}] {bytes}/{total_bytes} {bytes_per_sec}")
                    .unwrap()
                    .progress_chars("=> "),
            );
            pb.set_message(
                source
                    .file_name()
                    .unwrap_or(source.as_os_str())
                    .to_string_lossy()
                    .to_string(),
            );

            let file = fs::File::open(source)?;
            let mut reader = pb.wrap_read(BufReader::new(file));
            let mut writer = fs::File::create(dest)?;
            io::copy(&mut reader, &mut writer)?;
            pb.finish_and_clear();

            fs::remove_file(source)?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn move_file(
    source_file: &Path,
    dest_file: &Path,
    checksum: bool,
    args: Arc<ArgMatches>,
    multi: &MultiProgress,
) -> Result<()> {
    if source_file == dest_file {
        if args.get_flag("verbose") || args.get_flag("dry-run") {
            info!("{} is already in place, skipping.", source_file.display());
        }
    } else if dest_file.exists() {
        let source_size = source_file
            .metadata()
            .with_context(|| {
                format!("Unable to read size of '{}'.", source_file.display())
            })?
            .len();
        let dest_size = fs::File::open(dest_file)
            .with_context(|| {
                format!("Unable to open '{}'.", dest_file.display())
            })?
            .metadata()
            .with_context(|| {
                format!("Unable to read size of '{}'.", dest_file.display())
            })?
            .len();

        let is_duplicate = files_match(
            source_file,
            dest_file,
            source_size,
            dest_size,
            checksum,
        )?;

        if is_duplicate {
            if args.get_flag("remove-source") && !args.get_flag("dry-run") {
                fs::remove_file(source_file).with_context(|| {
                    format!("Failed to remove {}.", source_file.display())
                })?;
                info!("Removed {}.", source_file.display());
            } else if args.get_flag("trash-source") && !args.get_flag("dry-run")
            {
                trash::delete(source_file).with_context(|| {
                    format!("Failed to trash {}.", source_file.display())
                })?;
                info!("Trashed {}.", source_file.display());
            } else if args.get_flag("verbose") || args.get_flag("dry-run") {
                let method = if checksum { "checksum" } else { "size" };
                info!(
                    "{} exists with matching {}; skipping {}.",
                    dest_file.display(),
                    method,
                    source_file.display()
                );
            }
        } else if args.get_flag("verbose") || args.get_flag("dry-run") {
            let method = if checksum { "content" } else { "size" };
            info!(
                "{} exists with different {}; not moving {}.",
                dest_file.display(),
                method,
                source_file.display()
            );
        }
    } else {
        // Move file.
        if args.get_flag("verbose") || args.get_flag("dry-run") {
            info!("{} ➔ {}", source_file.display(), dest_file.display());
        }
        if !args.get_flag("dry-run") {
            move_or_copy(source_file, dest_file, multi).with_context(|| {
                format!(
                    "Unable to move {} to {}.",
                    source_file.display(),
                    dest_file.display()
                )
            })?
        }
    }

    Ok(())
}
