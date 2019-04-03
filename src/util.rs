use std::env;
use std::fs;
use std::process::Command;

#[allow(dead_code)]
const EXTENSIONS: &'static [&'static str] = &[
    // RAW file extensions
    "3fr", "ari", "arw", "bay", "cap", "cr2", "cr3", "crw", "data", "dcr", "dcs", "dng", "drf",
    "eip", "erf", "fff", "gpr", "iiq", "k25", "kdc", "mdc", "mef", "mos", "mrw", "nef", "nrw",
    "obm", "orf", "pef", "ptx", "pxn", "r3d", "raf", "raw", "rw2", "rwl", "rwz", "sr2", "srf",
    "srw", "x3f", // other image files
    "fpx", "gif", "j2k", "jfif", "jif", "jp2", "jpeg", "jpg", "jpx", "pcd", "psd", "tif", "tiff",
    // movie file formats
    "264", "3g2", "3gp", "amv", "asf", "avi", "cine", "drc", "f4a", "f4b", "f4p", "f4v", "flv", "flv",
    "gifv", "m2ts", "m2v", "m4p", "m4v", "mkv", "mng", "mp4", "mpeg", "mpg", "mts", "mxf", "nsv",
    "ogg", "qt", "roq", "svi", "vob", "wmv", "yuv",
];

fn is_image(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| {
            for ext in EXTENSIONS {
                if s.ends_with((String::from(".") + ext).as_str()) {
                    return true;
                }
            }
            false
        })
        .unwrap_or(false)
}

fn is_program_in_path(program: &str) -> bool {
    if let Ok(path) = env::var("PATH") {
        for p in path.split(":") {
            let p_str = format!("{}/{}", p, program);
            if fs::metadata(p_str).is_ok() {
                return true;
            }
        }
    }
    false
}

fn remove_file(file: &Path, use_rip: bool) -> Result<()> {

    if use_rip && is_program_in_path( "rip" ) {
        println!("Using rip to remove file safely");
        Command::new("rip")
            .arg( file.as_os_str() )
            .output()
            .expect(format!("Failed to execute external 'rip' command to remove '{}'.", file.display()).as_str());
    } 
    else {
        fs::remove_file( file ).chain_err(|| format!("Failed to remove '{}'.", file.display()))?;
    }

    Ok(())
}
