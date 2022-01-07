// -*- compile-command: "cargo build" -*-
#![recursion_limit = "1024"]

#[macro_use]
extern crate error_chain;

use chrono::NaiveTime;
use clap::{App, Arg, ArgMatches};
use exif::{DateTime, Tag, Value};
use std::{
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        ParseInt(::std::num::ParseIntError);
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
    let args = App::new("exifmv")
        .version("0.1.0")
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Moves images into a folder hierarchy based on EXIF DateTime tags")
        //.setting(AppSettings::NextLineHelp)
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Babble a lot"),
        )
        .arg(
            Arg::new("recurse")
                .short('r')
                .long("recurse-subdirs")
                .help("Recurse subdirectories"),
        )
        .arg(
            Arg::new("remove_source_if_target_exists")
                .long("remove-existing-source-files")
                .help("Remove any SOURCE file existing at DESTINATION and matching in size"),
        )
        .arg(
            Arg::new("use_rip")
                .long("use-rip")
                .help("Use external rip (Rm ImProved) utility to remove source files"),
        )
        .arg(
            Arg::new("dry_run")
                .long("dry-run")
                .help("Do not move any files (forces --verbose)"),
        )
        .arg(
            Arg::new("make_names_lowercase")
                .short('l')
                .long("make-lowercase")
                .help("Change filename & extension to lowercase"),
        )
        .arg(
            Arg::new("dereference_symlinks")
                .short('L')
                .long("dereference")
                .help("Dereference symbolic links"),
        )
        .arg(
            Arg::new("halt")
                .short('H')
                .long("halt-on-errors")
                .help("Exit if any errors are encountered"),
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
                .takes_value(true)
                .help("The time at which the date wraps to the next day (default: 00:00 aka midnight)"),
        )
        .arg(
            Arg::new("SOURCE")
                .required(true)
                .help("Where to search for images"),
        )
        .arg(
            Arg::new("DESTINATION")
                .required(false)
                .help("Where to move the images (if omitted, images will be moved to current dir)"),
        )
        .get_matches();

    let source = args.value_of("SOURCE").unwrap_or(".");

    let time_offset =
        NaiveTime::parse_from_str(args.value_of("day_wrap").unwrap_or("0:0"), "%H:%M").chain_err(
            || {
                format!(
                    "Option --day-wrap {} is formatted incorrectly.",
                    args.value_of("day_wrap").unwrap()
                )
            },
        )?;

    //println!("{}", time_offset.hour());
    //println!("{}", time_offset.minute());

    for entry in WalkDir::new(source)
        .contents_first(true)
        .max_depth({
            if args.is_present("recurse") {
                std::usize::MAX
            } else {
                1
            }
        })
        .follow_links(args.is_present("dereference_symlinks"))
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
        .into_iter()
        .filter_entry(|e| e.file_type().is_dir() || has_image_extension(e))
    {
        let dir_entry = entry.unwrap();

        if !dir_entry.file_type().is_dir() {
            if let Err(e) = move_image(
                dir_entry.path(),
                Path::new(args.value_of("DESTINATION").unwrap_or(".")),
                time_offset,
                &args,
            ) {
                if args.is_present("halt") {
                    return Err(e);
                }
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
    if !path.exists() && !args.is_present("dry_run") {
        std::fs::create_dir_all(&path)
            .chain_err(|| format!("Unable to create destination folder '{}'.", path.display()))?;
    }

    let dest_file = path.join(if args.is_present("make_names_lowercase") {
        source_file
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_lowercase()
    } else {
        source_file.file_name().unwrap().to_str().unwrap().into()
    });

    move_file(source_file, &dest_file, args)?;

    // Move possible sidecar files
    //let source_xmp_file = source_file
    //    .with_extension(source_file.extension()?.to_str()?.to_owned() + ".xmp");

    //TODO: support uppercase extension XMP files.

    let mut source_xmp_file = PathBuf::from(source_file);
    source_xmp_file.push(".xmp");

    if source_xmp_file.exists() {
        let mut dest_xmp_file = dest_file;
        dest_xmp_file.push(".xmp");

        move_file(&source_xmp_file, &dest_xmp_file, args)?;
    }

    Ok(())
}
