#![recursion_limit = "1024"]
//! Moves images into a folder hierarchy based on EXIF tags.
//!
//! XMP sidecar files are also moved, if present.
//!
//! Currently the hierarchy is hard-wired into the tool as this suits my needs.
//! In the future this should be configured by a human-readable string
//! supporting regular expressions etc.
//!
//! For now the built-in string is this:
//!
//! `{destination}/{year}/{month}/{day}/{filename}.{extension}`
//!
//! For example, if you have an image shot on *Nov. 22. 2019* named
//! `Foo1234.ARW` it will end up as this folder hierarchy: `2019/11/22/foo1234.
//! arw`.
//!
//! # Safety
//!
//! With default settings `exifmv` uses move/rename only for organizing files.
//! The only thing you risk is having files end up somewhere you didn’t intend.
//!
//! But – if you specify the `--remove-source` flag and it
//! detects duplicates it will delete the original at the source. This is
//! triggered by files at the destination matching in name and size.
//!
//! **In this case the original is removed!**
//!
//! However, you can use the `--trash-source` flag instead and files are moved
//! to your system's graveyard/recycling bin instead of being permanently
//! deleted right away.
//!
//! All that being said: I have been using this app since about four years
//! without loosing any images. As such I have quite a lot of _empirical_
//! evidence that it doesn’t destroy data.
//!
//! Still – writing some proper tests would likely give everyone else more
//! confidence than my word. Until I find some time to do that: **you have been
//! warned.**
//!
//! # Usage
//!
//! ```text
//! USAGE:
//!     exifmv [OPTIONS] <SOURCE> [DESTINATION]
//!
//! ARGS:
//!     <SOURCE>         Where to search for images
//!     <DESTINATION>    Where to move the images [default: .]
//!
//! OPTIONS:
//!         --day-wrap <H[H][:M[M]]>    The time at which the date wraps to the next day [default: 0:0]
//!         --dry-run                   Do not move any files (forces --verbose)
//!     -h, --help                      Print help information
//!     -H, --halt-on-errors            Exit if any errors are encountered
//!     -l, --make-lowercase            Change filename & extension to lowercase
//!     -L, --dereference               Dereference symbolic links
//!     -r, --recurse-subdirs           Recurse subdirectories
//!         --remove-source             Delete any SOURCE file existing at DESTINATION and matching in
//!                                     size
//!         --trash-source              Move any SOURCE file existing at DESTINATION and matching in
//!                                     size to the system's trash
//!     -v, --verbose                   Babble a lot
//!     -V, --version                   Print version information
//! ```
//!
//! # History
//!
//! This is based on a Python script that did more or less the same thing and
//! which served me well for 15 years. When I started to learn Rust in 2018 I
//! decided to port the Python code to Rust as CLI app learning experience.
//!
//! As such this app may not be the prettiest code you've come across lately.
//! It may also contain non-idiomatic (aka: non-Rust) ways of doing stuff. If
//! you feel like fixing any of those or add some nice features, I look forward
//! to merge your PRs. Beers!
use anyhow::{Context, Result, anyhow};
use chrono::{Datelike, Days, NaiveDate, NaiveTime, Timelike};
#[cfg(feature = "color")]
use clap::builder::styling::{AnsiColor, Styles};
use clap::{Arg, ArgAction, ArgMatches, arg, command};
use exif::{DateTime, Tag, Value};
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;
use log::{info, warn};
use rayon::prelude::*;
use simplelog::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::{DirEntry, WalkDir};

mod config;
mod template;
#[cfg(test)]
mod tests;
mod util;

use config::Config as AppConfig;
use template::{Template, TemplateContext};
use util::*;

#[cfg(feature = "color")]
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Red.on_default())
    .error(AnsiColor::Red.on_default().bold());

fn main() -> Result<()> {
    // Get default config path for help text.
    let default_config_path =
        confy::get_configuration_file_path("exifmv", "config")
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "~/.config/exifmv/config.toml".into());
    let config_help =
        format!("Config file path [default: {default_config_path}]");

    #[cfg(feature = "color")]
    let cmd = command!().styles(STYLES);
    #[cfg(not(feature = "color"))]
    let cmd = command!();

    let args = cmd
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Moves images into a folder hierarchy based on EXIF DateTime tags")
        .arg(
            arg!(-v --verbose "Babble a lot").action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("recursive")
                .short('r')
                .long("recursive")
                .help("Recurse subdirectories")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("trash-source")
                .long("trash-source")
                .conflicts_with("remove-source")
                .help("Move any SOURCE file existing at DESTINATION and matching in size to the system's trash")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("remove-source")
                .long("remove-source")
                .conflicts_with("trash-source")
                .help("Delete source files that already exist at the destination")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Do not move any files (forces --verbose)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("make-lowercase")
                .short('l')
                .long("make-lowercase")
                .help("Change filename & extension to lowercase")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dereference-symlinks")
                .short('L')
                .long("dereference")
                .help("Dereference symbolic links")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("halt")
                .short('H')
                .long("halt-on-errors")
                .help("Exit if any errors are encountered")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("checksum")
                .long("checksum")
                .help("Verify file contents for duplicate detection")
                .action(ArgAction::SetTrue),
        )
        /*.arg(
            Arg::new("cleanup")
                .short("c")
                .long("cleanup")
                .help("Remove empty directories (including hidden files)"),
        )*/
       .arg(
            Arg::new("day-wrap")
                .long("day-wrap")
                .value_name("H[H][:M[M]]")
                .help("The time at which the date wraps to the next day"),
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .value_name("TEMPLATE")
                .help("Path format template (e.g., {year}/{month}/{day}/{filename}.{extension})"),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("PATH")
                .help(config_help),
        )
        .arg(
            Arg::new("SOURCE")
                .required(true)
                .help("Where to search for images"),
        )
        .arg(
            Arg::new("DESTINATION")
                .required(false)
                .default_value(".")
                .help("Where to move the images"),
        )
        .get_matches();

    // Load config file.
    let config_path = args.get_one::<String>("config").map(PathBuf::from);
    let app_config = AppConfig::load(config_path.as_ref())?;

    // Merge CLI args with config (CLI wins).
    let verbose = args.get_flag("verbose")
        || args.get_flag("dry-run")
        || app_config.verbose.unwrap_or(false);
    let recursive =
        args.get_flag("recursive") || app_config.recursive.unwrap_or(false);
    let make_lowercase = args.get_flag("make-lowercase")
        || app_config.make_lowercase.unwrap_or(false);
    let halt =
        args.get_flag("halt") || app_config.halt_on_errors.unwrap_or(false);
    let dereference = args.get_flag("dereference-symlinks")
        || app_config.dereference.unwrap_or(false);
    let checksum =
        args.get_flag("checksum") || app_config.checksum.unwrap_or(false);

    let multi = MultiProgress::new();
    let logger = TermLogger::new(
        if verbose {
            LevelFilter::Info
        } else {
            LevelFilter::Warn
        },
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    );
    LogWrapper::new(multi.clone(), logger).try_init().unwrap();

    // Parse day-wrap time.
    let day_wrap_str = args
        .get_one::<String>("day-wrap")
        .map(String::as_str)
        .or(app_config.day_wrap.as_deref())
        .unwrap_or("00:00");
    let time_offset = NaiveTime::parse_from_str(day_wrap_str, "%H:%M")
        .with_context(|| {
            format!(
                "Option --day-wrap {} is formatted incorrectly.",
                day_wrap_str
            )
        })?;

    // Parse and validate template.
    let format_str = args
        .get_one::<String>("format")
        .map(String::as_str)
        .unwrap_or_else(|| app_config.format());
    let template = Template::parse(format_str)?;
    template.validate()?;

    let source: &String = args.get_one("SOURCE").unwrap();
    let dest_dir =
        PathBuf::from(args.get_one::<String>("DESTINATION").unwrap());

    let files = WalkDir::new(source)
        .contents_first(true)
        .max_depth(if recursive { usize::MAX } else { 1 })
        .follow_links(dereference)
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .into_iter()
        .filter_entry(is_not_hidden)
        .filter_map(|e| {
            e.ok()
                .filter(|e| e.file_type().is_file() && has_image_extension(e))
        })
        .collect::<Vec<_>>();

    let args = Arc::new(args);
    let template = Arc::new(template);
    let multi = Arc::new(multi);

    let errors: Vec<_> = files
        .par_iter()
        .filter_map(|file| {
            let result = move_image(
                file.path(),
                &dest_dir,
                &time_offset,
                &template,
                make_lowercase,
                checksum,
                args.clone(),
                multi.clone(),
            );
            match result {
                Ok(()) => None,
                Err(e) => {
                    warn!("{}", e);
                    Some(e)
                }
            }
        })
        .collect();

    if halt && !errors.is_empty() {
        Err(anyhow!("{} error(s) encountered.", errors.len()))
    } else {
        Ok(())
    }
}

fn is_not_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| entry.depth() == 0 || !s.starts_with('.'))
        .unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn move_image(
    source_file: &Path,
    dest_dir: &Path,
    time_offset: &NaiveTime,
    template: &Template,
    make_lowercase: bool,
    checksum: bool,
    args: Arc<ArgMatches>,
    multi: Arc<MultiProgress>,
) -> Result<()> {
    let source_file_handle =
        std::fs::File::open(source_file).with_context(|| {
            format!("Unable to open '{}'.", source_file.display())
        })?;

    let exif_reader = exif::Reader::new();
    let meta_data = exif_reader
        .read_from_container(&mut std::io::BufReader::new(&source_file_handle))
        .with_context(|| {
            format!(
                "Unable to read EXIF metadata of '{}'.",
                source_file.display()
            )
        })?;

    let time_stamp = meta_data
        .get_field(Tag::DateTimeOriginal, exif::In::PRIMARY)
        .and_then(|f| match f.value {
            Value::Ascii(ref vec) if !vec.is_empty() => {
                DateTime::from_ascii(&vec[0]).ok()
            }
            _ => None,
        })
        .with_context(|| {
            format!(
                "Timestamp metadata missing in '{}'.",
                source_file.display()
            )
        })?;

    let date = NaiveDate::from_ymd_opt(
        time_stamp.year as i32,
        time_stamp.month as u32,
        time_stamp.day as u32,
    )
    .with_context(|| {
        format!(
            "Invalid date {}-{}-{} in '{}'.",
            time_stamp.year,
            time_stamp.month,
            time_stamp.day,
            source_file.display()
        )
    })?;

    let date = if day_wrap(&time_stamp, time_offset) == 1 {
        date.checked_add_days(Days::new(1)).with_context(|| {
            format!("Date overflow for '{}'.", source_file.display())
        })?
    } else {
        date
    };

    // Extract filename and extension.
    let file_stem = source_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let extension = source_file
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    // Build template context.
    let ctx = TemplateContext {
        year: format!("{}", date.year()),
        month: format!("{:02}", date.month()),
        day: format!("{:02}", date.day()),
        hour: format!("{:02}", time_stamp.hour),
        minute: format!("{:02}", time_stamp.minute),
        second: format!("{:02}", time_stamp.second),
        filename: if make_lowercase {
            file_stem.to_lowercase()
        } else {
            file_stem.to_string()
        },
        extension: if make_lowercase {
            extension.to_lowercase()
        } else {
            extension.to_string()
        },
        camera_make: get_exif_string(&meta_data, Tag::Make),
        camera_model: get_exif_string(&meta_data, Tag::Model),
        lens: get_exif_string(&meta_data, Tag::LensModel),
        iso: get_exif_string(&meta_data, Tag::PhotographicSensitivity),
        focal_length: get_exif_string(&meta_data, Tag::FocalLength),
    };

    // Expand template to get relative path.
    let relative_path = template.expand(&ctx);
    let mut dest_file = dest_dir.join(&relative_path);

    // Create parent directories.
    if let Some(parent) = dest_file.parent()
        && !args.get_flag("dry-run")
        && !parent.exists()
    {
        info!("Creating folder {}", parent.display());

        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Unable to create destination folder '{}'.",
                parent.display()
            )
        })?;
    }

    move_file(source_file, &dest_file, checksum, args.clone(), &multi)?;

    // Move possible sidecar files.
    let source_xmp_file = source_file.to_path_buf();

    let mut source_xmp_file_lower = source_xmp_file.clone();
    source_xmp_file_lower.as_mut_os_string().push(".xmp");

    let mut source_xmp_file_upper = source_xmp_file.clone();
    source_xmp_file_upper.as_mut_os_string().push(".XMP");

    if source_xmp_file_lower.exists() {
        dest_file.as_mut_os_string().push(".xmp");

        move_file(&source_xmp_file_lower, &dest_file, checksum, args, &multi)?;
    } else if source_xmp_file_upper.exists() {
        if make_lowercase {
            dest_file.as_mut_os_string().push(".xmp");
        } else {
            dest_file.as_mut_os_string().push(".XMP");
        };

        move_file(&source_xmp_file_upper, &dest_file, checksum, args, &multi)?;
    }

    Ok(())
}

/// Extract a string value from EXIF metadata.
fn get_exif_string(meta_data: &exif::Exif, tag: Tag) -> Option<String> {
    meta_data
        .get_field(tag, exif::In::PRIMARY)
        .map(|f| f.display_value().to_string().trim().to_string())
        .filter(|s| !s.is_empty())
}

pub(crate) fn day_wrap(time_stamp: &DateTime, time_offset: &NaiveTime) -> u8 {
    // Hour wrap.
    if time_stamp.hour as u32 + time_offset.hour() + {
        // Minute wrap.
        if time_stamp.minute as u32 + time_offset.minute() > 59 {
            1
        } else {
            0
        }
    } > 23
    {
        1
    } else {
        0
    }
}

#[test]
fn test_day_wrap() {
    assert_eq!(
        1,
        day_wrap(
            &DateTime {
                year: 2023,
                month: 8,
                day: 21,
                hour: 23,
                minute: 59,
                second: 0,
                nanosecond: None,
                offset: None,
            },
            &NaiveTime::from_hms_opt(0, 1, 0).unwrap(),
        ),
    );

    assert_eq!(
        0,
        day_wrap(
            &DateTime {
                year: 2023,
                month: 8,
                day: 21,
                hour: 23,
                minute: 59,
                second: 0,
                nanosecond: None,
                offset: None,
            },
            &NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        ),
    );
}
