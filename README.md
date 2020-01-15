# exifmv

Moves images into a folder hierarchy based on EXIF tags.

Currently the hierarchy is hard-wired into the tool as this suits my needs.
In the future this should be configured by a human-readable string supporting regular expressions etc.

For now the built-in string is this:
`{destination}/{year}/{month}/{day}/{filename}.{extension   }`

So if you have an image shot on *Nov. 22. 2019* named `Foo1234.ARW` it will end up as this folder hierarchy:
`2019/11/22/foo1234.arw`.


## Building

```
cargo build --release
```

## Usage

```
USAGE:
    exivmv [FLAGS] [ARGS]

FLAGS:
    -c, --cleanup            Clean up removing empty directories (incl. hidden files)
    -H, --halt-on-errors     Exit if any errors are encountered
    -l, --make-lowercase     Change filename & extension to lowercase
    -R, --recurse-subdirs    Recurse subdirectories
    -L, --follow-symlinks    Follow symbolic links
    -v, --verbose            Babble a lot
    -h, --help               Prints help information
    -V, --version            Prints version information

ARGS:
    <SOURCE>         Where to search for images
    <DESTINATION>    Where to move the images
```
