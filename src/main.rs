#![recursion_limit = "1024"]
//! Moves images into a folder hierarchy based on EXIF tags.
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
//! But – if you specify the `--remove-existing-source-files` flag and it
//! detects duplicates it will delete the original at the source. This is
//! triggered by files at the destination matching in name and size.
//!
//! **In this case the original is removed!**
//!
//! However, you can use [Rm ImProved](https://github.com/nivekuil/rip) by
//! specifying the `--use-rip` flag. This requires aforementioned tool to be
//! installed on your machine. When `rip` is used, files are moved to your
//! graveyard/recycling bin instead of being permanently deleted right away.
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
use chrono::NaiveTime;
use clap::{arg, command, Arg, ArgAction, ArgMatches};
use error_chain::error_chain;
use exif::{DateTime, Tag, Value};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

error_chain! {
    foreign_links {
        WalkDir(walkdir::Error);
        Io(std::io::Error);
        Trash(trash::Error);
        ParseInt(::std::num::ParseIntError);
        CommandLine(clap::parser::MatchesError);
    }
}

mod util;
use util::*;

fn main() {
    if let Err(ref e) = run() {
        eprintln!("error: {}", e);

        for e in e.iter().skip(1) {
            eprintln!("caused by: {}", e);
        }

        if let Some(backtrace) = e.backtrace() {
            eprintln!("backtrace: {:?}", backtrace);
        }

        ::std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = command!()
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Moves images into a folder hierarchy based on EXIF DateTime tags")
        //.setting(AppSettings::NextLineHelp)
        .arg(
            arg!(-v --verbose "Babble a lot").action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("recurse")
                .short('r')
                .long("recurse-subdirs")
                .help("Recurse subdirectories")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("trash_source")
                .long("trash-source")
                .conflicts_with("remove_source")
                .help("Move any SOURCE file existing at DESTINATION and matching in size to the system's trash")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("remove_source")
                .long("remove-source")
                .conflicts_with("trash_source")
                .help("Delete any SOURCE file existing at DESTINATION and matching in size")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry_run")
                .long("dry-run")
                .help("Do not move any files (forces --verbose)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("make_names_lowercase")
                .short('l')
                .long("make-lowercase")
                .help("Change filename & extension to lowercase")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dereference_symlinks")
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
            Arg::new("day_wrap")
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

    let source: &String = args.get_one("SOURCE").unwrap();

    let day_wrap: &String = args.get_one("day_wrap").unwrap();
    let time_offset = NaiveTime::parse_from_str(day_wrap, "%H:%M")
        .chain_err(|| format!("Option --day-wrap {} is formatted incorrectly.", day_wrap))?;

    //println!("{}", time_offset.hour());
    //println!("{}", time_offset.minute());

    for entry in WalkDir::new(source)
        .contents_first(true)
        .max_depth({
            if args.contains_id("recurse") {
                usize::MAX
            } else {
                1
            }
        })
        .follow_links(args.contains_id("dereference_symlinks"))
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .into_iter()
        .filter_entry(|e| !e.file_type().is_dir() && has_image_extension(e))
    {
        let dir_entry = entry?;

        let dest_dir: &String = args.get_one("DESTINATION").unwrap();
        if let Err(e) = move_image(dir_entry.path(), Path::new(dest_dir), time_offset, &args) {
            if args.contains_id("halt") {
                return Err(e);
            }
        }
    }

    Ok(())
}

fn move_image(
    source_file: &Path,
    dest_dir: &Path,
    _time_offset: NaiveTime,
    args: &ArgMatches,
) -> Result<()> {
    let source_file_handle = std::fs::File::open(source_file)
        .chain_err(|| format!("Unable to open '{}'.", source_file.display()))?;

    let exif_reader = exif::Reader::new();
    let meta_data = exif_reader
        .read_from_container(&mut std::io::BufReader::new(&source_file_handle))
        .chain_err(|| {
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
        //.unwrap(),
        .chain_err(|| format!("Timestamp metadata missing in '{}'.", source_file.display()))?;

    let path = dest_dir
        .join(format!("{}", time_stamp.year))
        .join(format!("{:02}", time_stamp.month))
        .join(format!("{:02}", time_stamp.day));

    /* + {
            if time_stamp.hour + time_offset.hour() + {
                if time_stamp.minute + time_offset.minute() > 59 {
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
    ));*/

    // Create the destiantion
    if !path.exists() && !args.contains_id("dry_run") {
        std::fs::create_dir_all(&path)
            .chain_err(|| format!("Unable to create destination folder '{}'.", path.display()))?;
    }

    let file_name = source_file.file_name().unwrap();
    let dest_file = if args.contains_id("make_names_lowercase") {
        if let Some(name_str) = file_name.to_str() {
            path.join(name_str.to_lowercase())
        } else {
            path.join(file_name)
        }
    } else {
        path.join(file_name)
    };

    move_file(source_file, &dest_file, args)?;

    // Move possible sidecar files
    let source_xmp_file = PathBuf::from(source_file);
    let source_xmp_file_lower = source_xmp_file.clone().join(".xmp");
    let source_xmp_file_upper = source_xmp_file.clone().join(".XMP");

    if source_xmp_file_lower.exists() {
        move_file(&source_xmp_file_lower, &dest_file.join(".xmp"), args)?;
    } else if source_xmp_file_upper.exists() {
        move_file(
            &source_xmp_file_lower,
            &if args.contains_id("make_names_lowercase") {
                dest_file.join(".xmp")
            } else {
                dest_file.join(".XMP")
            },
            args,
        )?;
    }

    Ok(())
}
