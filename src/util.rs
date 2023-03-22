use crate::*;
use std::fs;

#[allow(dead_code)]
const EXTENSIONS: &[&str] = &[
    // RAW file extensions
    "3fr", "ari", "arw", "bay", "cap", "cr2", "cr3", "crw", "data", "dcr", "dcs", "dng", "drf",
    "eip", "erf", "fff", "gpr", "iiq", "k25", "kdc", "mdc", "mef", "mos", "mrw", "nef", "nrw",
    "obm", "orf", "pef", "ptx", "pxn", "r3d", "raf", "raw", "rw2", "rwl", "rwz", "sr2", "srf",
    "srw", "x3f", // other image files
    "fpx", "gif", "j2k", "jfif", "jif", "jp2", "jpeg", "jpg", "jpx", "pcd", "psd", "tif", "tiff",
    // movie file formats
    "264", "3g2", "3gp", "amv", "asf", "avi", "cine", "drc", "f4a", "f4b", "f4p", "f4v", "flv",
    "flv", "gifv", "m2ts", "m2v", "m4p", "m4v", "mkv", "mng", "mp4", "mpeg", "mpg", "mts", "mxf",
    "nsv", "ogg", "qt", "roq", "svi", "vob", "wmv", "yuv",
];

pub(crate) fn has_image_extension(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| {
            for ext in EXTENSIONS {
                if s.to_lowercase()
                    .ends_with((String::from(".") + ext).as_str())
                {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

fn remove_file(file: &Path, use_rip: bool) -> Result<()> {
    if use_rip {
        trash::delete(file).chain_err(|| format!("Failed to remove {}.", file.display()))?;
    } else {
        fs::remove_file(file).chain_err(|| format!("Failed to remove {}.", file.display()))?;
    }

    Ok(())
}

pub(crate) fn move_file(source_file: &Path, dest_file: &Path, args: &ArgMatches) -> Result<()> {
    if source_file == dest_file {
        if args.is_present("verbose") || args.is_present("dry_run") {
            println!("{} is already in place, skipping.", source_file.display());
        }
    //bail!();
    } else if dest_file.exists() {
        if source_file
            .metadata()
            .chain_err(|| format!("Unable to read size of '{}'.", source_file.display()))?
            .len()
            == std::fs::File::open(dest_file)
                .chain_err(|| format!("Unable to open '{}'.", source_file.display()))?
                .metadata()
                .chain_err(|| format!("Unable to read size of '{}'.", source_file.display()))?
                .len()
        {
            if args.is_present("remove_source_if_target_exists") && !args.is_present("dry_run") {
                remove_file(source_file, args.is_present("use_rip"))?;
            } else if args.is_present("verbose") || args.is_present("dry_run") {
                println!(
                    "{} exists and has different size; not moving {}.",
                    dest_file.display(),
                    source_file.display()
                );
            }
        }
    } else {
        // Move file
        if args.is_present("verbose") || args.is_present("dry_run") {
            println!("{} ➔ {}", source_file.display(), dest_file.display());
        }
        if !args.is_present("dry_run") {
            fs::rename(source_file, dest_file).chain_err(|| {
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
