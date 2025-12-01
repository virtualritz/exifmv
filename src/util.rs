use crate::*;
use log::info;
use std::fs;

const EXTENSIONS: &[&str] = &[
    // RAW file extensions.
    "3fr", "ari", "arw", "bay", "cap", "cr2", "cr3", "crw", "data", "dcr", "dcs", "dng", "drf",
    "eip", "erf", "fff", "gpr", "iiq", "k25", "kdc", "mdc", "mef", "mos", "mrw", "nef", "nrw",
    "obm", "orf", "pef", "ptx", "pxn", "r3d", "raf", "raw", "rw2", "rwl", "rwz", "sr2", "srf",
    "srw", "x3f", // Other image files.
    "avif", "bmp", "fpx", "gif", "heic", "heif", "j2k", "jfif", "jif", "jp2", "jpeg", "jpg", "jpx",
    "pcd", "png", "psd", "tif", "tiff", "webp", // Movie file formats.
    "264", "3g2", "3gp", "amv", "asf", "avi", "cine", "drc", "f4a", "f4b", "f4p", "f4v", "flv",
    "gifv", "m2ts", "m2v", "m4p", "m4v", "mkv", "mng", "mp4", "mpeg", "mpg", "mts", "mxf", "nsv",
    "ogg", "qt", "roq", "svi", "vob", "wmv", "yuv",
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

pub(crate) fn move_file(source_file: &Path, dest_file: &Path, args: Arc<ArgMatches>) -> Result<()> {
    if source_file == dest_file {
        if args.get_flag("verbose") || args.get_flag("dry-run") {
            info!("{} is already in place, skipping.", source_file.display());
        }
    } else if dest_file.exists() {
        let source_size = source_file
            .metadata()
            .with_context(|| format!("Unable to read size of '{}'.", source_file.display()))?
            .len();
        let dest_size = std::fs::File::open(dest_file)
            .with_context(|| format!("Unable to open '{}'.", dest_file.display()))?
            .metadata()
            .with_context(|| format!("Unable to read size of '{}'.", dest_file.display()))?
            .len();

        if source_size == dest_size {
            if args.get_flag("remove-source") && !args.get_flag("dry-run") {
                fs::remove_file(source_file)
                    .with_context(|| format!("Failed to remove {}.", source_file.display()))?;
                info!("Removed {}.", source_file.display());
            } else if args.get_flag("trash-source") && !args.get_flag("dry-run") {
                trash::delete(source_file)
                    .with_context(|| format!("Failed to trash {}.", source_file.display()))?;
                info!("Trashed {}.", source_file.display());
            } else if args.get_flag("verbose") || args.get_flag("dry-run") {
                info!(
                    "{} exists with matching size; skipping {}.",
                    dest_file.display(),
                    source_file.display()
                );
            }
        } else if args.get_flag("verbose") || args.get_flag("dry-run") {
            info!(
                "{} exists with different size; not moving {}.",
                dest_file.display(),
                source_file.display()
            );
        }
    } else {
        // Move file
        if args.get_flag("verbose") || args.get_flag("dry-run") {
            info!("{} ➔ {}", source_file.display(), dest_file.display());
        }
        if !args.get_flag("dry-run") {
            fs::rename(source_file, dest_file).with_context(|| {
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
