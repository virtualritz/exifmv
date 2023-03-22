
# `exifmv`

![build](https://github.com/virtualritz/exifmv/workflows/build/badge.svg)
![Maintenance](https://img.shields.io/badge/maintenance-passively--maintained-yellowgreen.svg)

Moves images into a folder hierarchy based on EXIF tags.

Currently the hierarchy is hard-wired into the tool as this suits my needs.
In the future this should be configured by a human-readable string
supporting regular expressions etc.

For now the built-in string is this:

`{destination}/{year}/{month}/{day}/{filename}.{extension}`

For example, if you have an image shot on *Aug. 15 2020* named
`Foo1234.ARW` it will e.g. end up in a folder hierarchy like so:

```
2020
├── 08
│   ├── 15
│   │   ├── foo1234.arw
│   │   ├── …
```

## Safety

With default settings `exifmv` uses move/rename only for organizing files.
The only thing you risk is having files end up somewhere you didn’t intend.

But – if you specify the `--remove-source` it will delete the original.
Before doing so it checks that the file size matches. This is not the same as
checking the files byte-by-byte but due to almost all image file formats using
some kind of compression matching size is a good enough indicator for files
being identical.

Alternatively you can use the `--trash-source` which will move source files to
the user's trash folder from where they can be restored to their original
location on most operating systems.

**In this case the original is removed!**

However, you can use [Rm ImProved (`rip`)](https://github.com/nivekuil/rip) by
specifying the `--use-rip` flag. This requires aforementioned tool to be
installed on your machine. When `rip` is used, files are moved to your
graveyard/recycling bin instead of being permanently deleted right away.

All that being said: I have been using this app since about four years
without loosing any images. As such I have quite a lot of _empirical_
evidence that it doesn’t destroy data.

Still – writing some proper tests would likely give everyone else more
confidence than my word. Until I find some time to do that: **you have been
warned.**

## Usage

```cli
USAGE:
    exifmv [OPTIONS] <SOURCE> [DESTINATION]

ARGS:
    <SOURCE>         Where to search for images
    <DESTINATION>    Where to move the images (if omitted, images will be moved to current dir)
                     [default: .]

OPTIONS:
        --day-wrap <H[H][:M[M]]>    The time at which the date wraps to the next day [default: 0:0]
        --dry-run                   Do not move any files (forces --verbose)
    -h, --help                      Print help information
    -H, --halt-on-errors            Exit if any errors are encountered
    -l, --make-lowercase            Change filename & extension to lowercase
    -L, --dereference               Dereference symbolic links
    -r, --recurse-subdirs           Recurse subdirectories
        --remove-source             Delete any SOURCE file existing at DESTINATION and matching in
                                    size
        --trash-source              Move any SOURCE file existing at DESTINATION and matching in
                                    size to the system's trash
    -v, --verbose                   Babble a lot
    -V, --version                   Print version information
```

## History

This is based on a Python script that did more or less the same thing and
which served me well for 15 years. When I started to learn Rust in 2018 I
decided to port the Python code to Rust as CLI app learning experience.

As such this app may not be the prettiest code you've come accross lately.
It may also contain non-idiomatic (aka: non-Rust) ways of doing stuff. If
you feel like fixing any of those or add some nice features, I look forward
to merge your PRs. Beers!

Current version: 0.1.2

## License

Apache-2.0 OR BSD-3-Clause OR MIT OR Zlib at your discretion.
