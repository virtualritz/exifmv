// -*- compile-command: "cargo build" -*-
#![recursion_limit = "1024"]

#[macro_use]
extern crate error_chain;

extern crate clap;
extern crate exif;
extern crate walkdir;

use clap::{App, Arg, ArgMatches};
use exif::DateTime;
use exif::Tag;
use exif::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::string::String;
use walkdir::WalkDir;

#[allow(deprecated)]
error_chain! {
    foreign_links {
        Io(std::io::Error);
        ParseInt(::std::num::ParseIntError);
    }
}

include!("util.rs");

fn main() {
    if let Err(ref e) = run() {
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "error: {}", e).expect(errmsg);

        for e in e.iter().skip(1) {
            writeln!(stderr, "caused by: {}", e).expect(errmsg);
        }

        if let Some(backtrace) = e.backtrace() {
            writeln!(stderr, "backtrace: {:?}", backtrace).expect(errmsg);
        }

        ::std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = App::new("mvimg")
        .version("0.1.0")
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Moves images into a folder hierarchy based on EXIF tags")
        .arg(
            Arg::with_name("SOURCE")
                .required(true)
                .help("Where to search for images"),
        )
        .arg(
            Arg::with_name("DESTINATION")
                .required(true)
                .help("Where to move the images"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Babble a lot"),
        )
        .arg(
            Arg::with_name("recurse")
                .short("r")
                .long("recurse-subdirs")
                .help("Recurse subdirectories"),
        )
        .arg(
            Arg::with_name("remove_source_if_target_exists")
                .long("remove-source-files")
                .help("Remove any SOURCE file existing at DESTINATION and matching in size"),
        )
        .arg(
            Arg::with_name("use_rip")
                .long("use-rip")
                .help("Use external rip (Rm ImProved) utility to remove source files"),
        )
        .arg(
            Arg::with_name("make_names_lowercase")
                .short("l")
                .long("make-lowercase")
                .help("Change filename to lowercase"),
        )
        .arg(
            Arg::with_name("dereference_symlinks")
                .short("L")
                .long("dereference")
                .help("Dereference symbolic links"),
        )
        .arg(
            Arg::with_name("halt")
                .short("H")
                .long("halt-on-errors")
                .help("Exit if any errors are encountered"),
        )
        .arg(
            Arg::with_name("cleanup")
                .short("c")
                .long("cleanup")
                .help("Clean up removing empty directories (incl. hidden files)"),
        )
        .get_matches();

    let source = args.value_of("SOURCE").unwrap_or(".");
    println!("The source folder is '{}'", source);

    for entry in WalkDir::new(source)
        .follow_links(args.is_present("dereference_symlinks"))
        .into_iter()
        .filter_entry(|e| is_image(e) || (args.is_present("recurse") && e.file_type().is_dir()))
    {
        let dir_entry = entry.unwrap();

        if !dir_entry.file_type().is_dir() {
            println!("{}", dir_entry.path().display());

            match move_image(
                dir_entry.path(),
                Path::new(args.value_of("DESTINATION").unwrap_or(".")),
                &args,
            ) {
                Err(e) => {
                    if args.is_present("halt") {
                        return Err(e);
                    }
                }
                Ok(_) => (),
            }
        }
    }

    Ok(())
}

fn move_image(source_file: &Path, dest_dir: &Path, args: &ArgMatches) -> Result<()> {
    let source_file_handle = std::fs::File::open(source_file)
        .chain_err(|| format!("Unable to open '{}'.", source_file.display()))?;

    let meta_data = exif::Reader::new(&mut std::io::BufReader::new(&source_file_handle))
        .chain_err(|| {
            format!(
                "Unable to read EXIF metadata of '{}'.",
                source_file.display()
            )
        })?;

    let time_stamp = exif::DateTime::from(
        meta_data
            .get_field(Tag::DateTimeOriginal, false)
            .and_then(|f| match f.value {
                Value::Ascii(ref vec) if !vec.is_empty() => DateTime::from_ascii(vec[0]).ok(),
                _ => None,
            })
            .unwrap(),
    );

    let path = dest_dir
        .join(format!("{}", time_stamp.year))
        .join(format!("{:02}", time_stamp.month))
        .join(format!("{:02}", time_stamp.day));

    // Create the destiantion
    if !path.exists() {
        std::fs::create_dir_all(&path)
            .chain_err(|| format!("Unable to create destination folder '{}'.", path.display()))?;
    }

    let dest_file = path.join(
        source_file
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_lowercase(),
    );

    println!("source file: '{}'", source_file.display());
    println!("destination file: '{}'", dest_file.display());

    if source_file == dest_file {
        if args.is_present("verbose") {
            println!("'{}' is already in place, skipping.", source_file.display());
        }
    //bail!();
    } else if dest_file.exists() {
        if source_file
            .metadata()
            .chain_err(|| format!("Unable to read size of '{}'.", source_file.display()))?
            .len()
            == std::fs::File::open(&dest_file)
                .chain_err(|| format!("Unable to open '{}'.", source_file.display()))?
                .metadata()
                .chain_err(|| format!("Unable to read size of '{}'.", source_file.display()))?
                .len()
        {
            if args.is_present("remove_source_if_target_exists") {
                remove_file(source_file, args.is_present("use_rip"))?;
            } else {
                if args.is_present("verbose") {
                    println!(
                        "'{}' xists and has different size; not moving '{}'.",
                        dest_file.display(),
                        source_file.display()
                    );
                }
            }
        }
    } else {
        // Move file
        fs::rename(source_file, dest_file).chain_err(|| {
            format!(
                "Unable to move '{}' to '{}'.",
                source_file.display(),
                dest_dir.display()
            )
        })?
    }

    // Move possible sidecar files
    //let source_xmp_file = source_file
    //    .with_extension(source_file.extension()?.to_str()?.to_owned() + ".xmp");

    let source_xmp_file = PathBuf::from({
        let mut tmp = source_file.as_os_str().to_owned();
        tmp.push(".xmp");
        tmp
    });

    //let dest_xmp_file = Path::new( dest_file.join(".xmp") );

    println!("source XMP file: '{}'", source_xmp_file.display());
    //println!("destination XMP file: '{}'", dest_xmp_file.display());

    Ok(())
}

/*if None != metaData and len( metaData ):
        try:
            timeStamp = metaData[ 'Exif.Photo.DateTimeOriginal' ].value
        except:
            try:
                timeStamp = metaData[ 'Exif.Image.DateTime' ].value
            except:
                try:
                    timeStamp = metaData[ 'Exif.Image.DateTimeDigitized' ].value
                except:
                    timeStamp = datetime.datetime.fromtimestamp( os.path.getmtime( sourceFile ) )
                    #print( sourceFile + " " + str( timeStamp ) )
                    #print( 'No metadata found that could be used to move "%s".' % sourceFile )
                    #continue

        if type( timeStamp ) == str:
            return

        path = os.path.join( destination, str( timeStamp.year ), '%02d' % timeStamp.month, '%02d' % timeStamp.day )

        if not os.path.exists( path ):
            os.makedirs( path )

        destinationFile = os.path.join( path, fileName.lower() )

        if sourceFile == destinationFile:
            print( '"%s" is already in place, skipping.' % sourceFile )
        elif os.path.exists( destinationFile ):
            if os.path.getsize( sourceFile ) == os.path.getsize( sourceFile ):
                print( '"%s" is a duplicate of "%s", deleting.' % ( sourceFile, destinationFile ) )
                os.remove( sourceFile )
            else:
                print( '"%s" exists and is different, not moving %s.' % ( destinationFile, sourceFile ) )
        else:
            if verbose:
                print( sourceFile + "\t-> " + destinationFile )
            try:
                shutil.move( sourceFile, destinationFile )
            except:
                print( 'Could not move "%s".' % sourceFile )

        sourceFileSideCar = sourceFile + '.xmp'
        destinationFileSideCar = destinationFile + '.xmp'

        if os.path.exists( sourceFileSideCar ):
            if sourceFileSideCar == destinationFileSideCar:
                print( '"%s" is already in place, skipping.' % sourceFileSideCar )
            elif os.path.exists( destinationFileSideCar ):
                if os.path.getsize( sourceFileSideCar ) == os.path.getsize( destinationFileSideCar ):
                    print( '"%s" is a duplicate of "%s", deleting.' % ( sourceFileSideCar, destinationFileSideCar ) )
                    os.remove( sourceFileSideCar )
                else:
                    print( '"%s" exists and is different, not moving %s.' % ( destinationFileSideCar, sourceFileSideCar ) )
            else:
                print( sourceFileSideCar + "\t-> " + destinationFileSideCar )
                try:
                    shutil.move( sourceFileSideCar, destinationFileSideCar )
                except:
                    print( 'Could not move "%s".' % sourceFile )
*/
