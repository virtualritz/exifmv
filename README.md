# `exifmv`

![build](https://github.com/virtualritz/exifmv/workflows/build/badge.svg)
![Maintenance](https://img.shields.io/badge/maintenance-passively--maintained-yellowgreen.svg)

Moves images into a folder hierarchy based on EXIF tags.

XMP sidecar files are also moved, if present.

The folder hierarchy is configurable via a template string
(`-f`/`--format`). The default template is:

`{year}/{month}/{day}/{filename}.{extension}`

Available template variables: `year`, `month`, `day`, `hour`, `minute`,
`second`, `filename`, `extension`, `camera_make`, `camera_model`, `lens`,
`iso`, `focal_length`.

Run `exifmv --help` for full variable descriptions and examples.

## Example

If you have an image shot on _Aug. 15 2020_ named
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

But – if you specify the `--remove-source` it will _remove the original_.

> **In this case the original is permanently deleted!**

Alternatively you can use the `--trash-source` which will move source files
to the user’s trash folder from where they can be restored to their original
location on most operating systems.

Before doing any deletion or moving-to-trash `exifmv` checks that the file
size matches. Use `--checksum` to verify file contents instead, eliminating
false positives from same-size different-content files.

## Configuration File

`exifmv` supports a TOML configuration file. The default location is
platform-specific (e.g., `~/.config/exifmv/config.toml` on Linux).

```toml
format = "{year}/{month}/{day}/{filename}.{extension}"
make-lowercase = true
recursive = true
day-wrap = "04:00"
verbose = false
halt-on-errors = false
dereference = false
checksum = false
```

CLI arguments override config file settings.

## Features

- **color** (default): Enables colored CLI help output. Disable with
  `--no-default-features`.

## History

This is based on a Python script that did more or less the same thing and
which served me well for 15 years. When I started to learn Rust in 2018 I
decided to port the Python code to Rust as CLI app learning experience.

As such this app may not be the prettiest code you’ve come across lately.
It may also contain non-idiomatic (aka: non-Rust) ways of doing stuff. If
you feel like fixing any of those or add some nice features, I look forward
to merge your PRs. Beers!

Current version: 0.5.3

## License

Apache-2.0 OR BSD-3-Clause OR MIT OR Zlib
