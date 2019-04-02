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
use std::path::Path;
use std::string::String;
use walkdir::WalkDir;

error_chain! {
    foreign_links {
        Io(std::io::Error);
        ParseInt(::std::num::ParseIntError);
    }
}

#[allow(dead_code)]
const EXTENSIONS: &'static [&'static str] = &["dng", "rw2", "arw", "jpg", "jpeg", "psd", "avi"];

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

fn move_image(source_file: &Path, destination_dir: &Path, _verbose: bool) -> Result<()> {
    println!("sourcefile: '{}'", source_file.display());

    let file = std::fs::File::open(source_file).unwrap();
    let meta_data = exif::Reader::new(&mut std::io::BufReader::new(&file)).unwrap();

    let time_stamp = exif::DateTime::from(
        meta_data
            .get_field(Tag::DateTimeOriginal, false)
            .and_then(|f| match f.value {
                Value::Ascii(ref vec) if !vec.is_empty() => DateTime::from_ascii(vec[0]).ok(),
                _ => None,
            })
            .unwrap(),
    );

    let path = destination_dir
        .join(format!("{}", time_stamp.year))
        .join(format!("{:02}", time_stamp.month))
        .join(format!("{:02}", time_stamp.day));

    println!("destination: '{}'", path.display());

    if !path.exists() {
        std::fs::create_dir_all(&path)
            .chain_err(|| format!("Unable to create destination folder '{}'.", path.display()))?;
    }

    let destination_file = destination_dir.join(String::from(source_file.to_str().unwrap()).to_lowercase());

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

fn run(args: &ArgMatches) -> Result<()> {
    let source = args.value_of("SOURCE").unwrap_or(".");
    println!("The source folder is '{}'", source);

    for entry in WalkDir::new(source)
        .follow_links(args.is_present("SYMLINKS"))
        .into_iter()
        .filter_entry(|e| is_image(e) || e.file_type().is_dir())
    {
        let dir_entry = entry.unwrap();

        if !dir_entry.file_type().is_dir() {
            println!("{}", dir_entry.path().display());

            move_image(
                dir_entry.path(),
                Path::new(args.value_of("DESTINATION").unwrap_or(".")),
                args.is_present("VERBOSE"),
            )?;
        }
    }

    Ok(())
}

fn main() {
    let args = App::new("mvimg")
        .version("0.1.0")
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Moves images into a folder hierarchy based on EXIF tags")
        .arg(
            Arg::with_name("SOURCE")
                .required(false)
                .help("Where to search for images"),
        )
        .arg(
            Arg::with_name("DESTINATION")
                .required(false)
                .help("Where to move the images"),
        )
        .arg(
            Arg::with_name("VERBOSE")
                .short("v")
                .long("verbose")
                .help("Babble a lot"),
        )
        .arg(
            Arg::with_name("RECURSE")
                .short("r")
                .short("R")
                .long("recurse-subdirs")
                .help("Recurse subdirectories"),
        )
        .arg(
            Arg::with_name("LOWERCASE")
                .short("l")
                .long("make-lowercase")
                .help("Change filename to lowercase"),
        )
        .arg(
            Arg::with_name("SYMLINKS")
                .short("L")
                .long("follow-symlinks")
                .help("Follow symbolic links"),
        )
        .arg(
            Arg::with_name("HALT")
                .short("H")
                .long("halt-on-errors")
                .help("Exit if any errors are encountered"),
        )
        .arg(
            Arg::with_name("CLEANUP")
                .short("c")
                .long("cleanup")
                .help("Clean up removing empty directories (incl. hidden files)"),
        )
        .get_matches();

    if let Err(ref e) = run(&args) {
        println!("error: {}", e);
        for e in e.iter().skip(1) {
            println!("caused by: {}", e);
        }
        if let Some(backtrace) = e.backtrace() {
            println!("backtrace: {:?}", backtrace);
        }
        std::process::exit(1);
    }
}
