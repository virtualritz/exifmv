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
//!     <DESTINATION>    Where to move the images (if omitted, images will be moved to current dir)
//!                      [default: .]
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
use anyhow::{Context, Result};
use chrono::{NaiveTime, Timelike};
use clap::{arg, command, Arg, ArgAction, ArgMatches};
use exif::{DateTime, Tag, Value};
use log::{info, warn};
use simplelog::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use walkdir::{DirEntry, WalkDir};

mod util;
use util::*;

#[tokio::main]
async fn main() -> Result<()> {
    let args = command!()
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Moves images into a folder hierarchy based on EXIF DateTime tags")
        //.setting(AppSettings::NextLineHelp)
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
                .help("Move any SOURCE file existing at DESTINATION and matching in size to the system’s trash")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("remove-source")
                .long("remove-source")
                .conflicts_with("trash-source")
                .help("Delete any SOURCE file existing at DESTINATION and matching in size")
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
                .default_value("0:0")
                .help("The time at which the date wraps to the next day"),
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
                .help("Where to move the images (if omitted, images will be moved to current dir)"),
        )
        .get_matches();

    CombinedLogger::init(vec![TermLogger::new(
        if args.get_flag("verbose") {
            LevelFilter::Info
        } else {
            LevelFilter::Warn
        },
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])?;

    let source: &String = args.get_one("SOURCE").unwrap();

    let day_wrap: &String = args.get_one("day-wrap").unwrap();
    let time_offset = NaiveTime::parse_from_str(day_wrap, "%H:%M")
        .with_context(|| format!("Option --day-wrap {} is formatted incorrectly.", day_wrap))?;

    let dest_dir = PathBuf::from(args.get_one::<String>("DESTINATION").unwrap());

    for file in WalkDir::new(source)
        .contents_first(true)
        .max_depth({
            if args.get_flag("recursive") {
                usize::MAX
            } else {
                1
            }
        })
        .follow_links(args.get_flag("dereference-symlinks"))
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .into_iter()
        .filter_entry(is_not_hidden)
        .filter(|e| {
            e.as_ref()
                .map_or(false, |e| e.file_type().is_file() && has_image_extension(e))
        })
    {
        // We filtered out errors above so this unwrap can't fail.
        let file = file?;

        let args = Arc::new(args.clone());

        let dest_dir = dest_dir.clone();

        tokio::spawn(async move {
            if let Err(e) = move_image(file.path(), dest_dir, &time_offset, args.clone()).await {
                if args.get_flag("halt") {
                    return Err(e);
                } else {
                    warn!("{}", e);
                }
            }
            Ok(())
        });
    }

    Ok(())
}

fn is_not_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| entry.depth() == 0 || !s.starts_with('.'))
        .unwrap_or(false)
}

async fn move_image(
    source_file: &Path,
    dest_dir: PathBuf,
    time_offset: &NaiveTime,
    args: Arc<ArgMatches>,
) -> Result<()> {
    let source_file_handle = std::fs::File::open(source_file)
        .with_context(|| format!("Unable to open '{}'.", source_file.display()))?;

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
            Value::Ascii(ref vec) if !vec.is_empty() => DateTime::from_ascii(&vec[0]).ok(),
            _ => None,
        })
        .with_context(|| format!("Timestamp metadata missing in '{}'.", source_file.display()))?;

    let path = dest_dir
        .join(format!("{}", time_stamp.year))
        .join(format!("{:02}", time_stamp.month))
        .join(format!(
            "{:02}",
            time_stamp.day + calc_time_wrap(&time_stamp, &time_offset)
        ));

    // Create the destiantion.
    if !args.get_flag("dry-run") && !path.exists() {
        info!("Creating folder {}", path.display());

        std::fs::create_dir_all(&path).with_context(|| {
            format!("Unable to create destination folder '{}'.", path.display())
        })?;
    }

    let file_name = source_file.file_name().unwrap();
    let dest_file = if args.get_flag("make-lowercase") {
        if let Some(name_str) = file_name.to_str() {
            path.join(name_str.to_lowercase())
        } else {
            path.join(file_name)
        }
    } else {
        path.join(file_name)
    };

    move_file(source_file, &dest_file, args.clone())?;

    // Move possible sidecar files.
    let source_xmp_file = PathBuf::from(source_file);
    let source_xmp_file_lower = source_xmp_file.clone().join(".xmp");
    let source_xmp_file_upper = source_xmp_file.clone().join(".XMP");

    if source_xmp_file_lower.exists() {
        move_file(&source_xmp_file_lower, &dest_file.join(".xmp"), args)?;
    } else if source_xmp_file_upper.exists() {
        move_file(
            &source_xmp_file_lower,
            &if args.get_flag("make-lowercase") {
                dest_file.join(".xmp")
            } else {
                dest_file.join(".XMP")
            },
            args,
        )?;
    }

    Ok(())
}

fn calc_time_wrap(time_stamp: &DateTime, time_offset: &NaiveTime) -> u8 {
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
fn test_calc_time_wrap() {
    assert_eq!(
        1,
        calc_time_wrap(
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
        calc_time_wrap(
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
