# geohash-prepper

Turns a CSV of camera coordinates into the deduplicated geohash table that the
firmware compiles in.

It finds the latitude and longitude columns by header name (`latitude`/`lat`/`y`
and `longitude`/`lon`/`lng`/`long`/`x`, case-insensitive), encodes each point to a
precision-7 geohash, drops duplicates, and writes `geohash,latitude,longitude`.

Precision 7 is roughly a 150 m × 150 m cell. The firmware checks the cell you are
in plus its eight neighbours, so the search area is about 450 m across — wider
than the 800 ft alert radius, which is what makes the distance check meaningful.

## Usage

```bash
cargo run -- <input.csv> [output.csv]
```

Defaults: `input.csv` in, `geodata.csv` out.

To regenerate the table the firmware ships with:

```bash
cargo run -- data/Chicago_Speed_Camera_Locations_20250911.csv ../../firmware/data/geodata.csv
```

Then rebuild the firmware — `build.rs` reads that file and generates a perfect
hash map from it at compile time.

## Input data

`data/Chicago_Speed_Camera_Locations_20250911.csv` is the City of Chicago's
open-data speed camera list, retrieved 2025-09-11.

Any CSV with recognisable latitude/longitude headers works. If you want a
different city, find its open-data equivalent — most large US cities publish
one — and point the tool at it.
