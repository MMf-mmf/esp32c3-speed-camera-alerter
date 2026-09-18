use geohash::{Coord, encode};
use std::collections::HashSet;
use std::error::Error;

// A struct for writing the clean, processed data to our output CSV.
#[derive(Debug, serde::Serialize)]
struct OutputRecord<'a> {
    geohash: &'a str,
    latitude: f64,
    longitude: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Define the input and output file paths.
    let input_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "input.csv".to_string());
    let output_path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "geodata.csv".to_string());

    // Create a CSV reader for the input file with flexible parsing.
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true) // Allow rows with varying number of fields
        .from_path(&input_path)?;

    // Get headers and find latitude/longitude column indices
    let headers = reader.headers()?.clone();
    let (lat_idx, lon_idx) = find_lat_lon_columns(&headers)?;

    println!(
        "Found latitude in column '{}', longitude in column '{}'",
        &headers[lat_idx], &headers[lon_idx]
    );

    // Create a CSV writer for the new output file.
    let mut writer = csv::Writer::from_path(&output_path)?;

    // Define the precision for the geohash.
    const GEOHASH_PRECISION: usize = 7;

    // Track unique geohashes to avoid duplicates
    let mut seen_geohashes: HashSet<String> = HashSet::new();

    println!("Processing CSV file: '{}'", input_path);

    // Iterate over each record in the input CSV.
    for result in reader.records() {
        let record = result?;

        // Parse latitude and longitude from the detected columns
        let latitude: f64 = match record.get(lat_idx) {
            Some(val) => match val.trim().parse() {
                Ok(v) => v,
                Err(_) => {
                    eprintln!("Skipping row: invalid latitude '{}'", val);
                    continue;
                }
            },
            None => continue,
        };

        let longitude: f64 = match record.get(lon_idx) {
            Some(val) => match val.trim().parse() {
                Ok(v) => v,
                Err(_) => {
                    eprintln!("Skipping row: invalid longitude '{}'", val);
                    continue;
                }
            },
            None => continue,
        };

        // Create a `Coord` struct required by the geohash crate.
        let coord = Coord {
            x: longitude,
            y: latitude,
        };

        // Encode the coordinate to a geohash string.
        let geohash_str = encode(coord, GEOHASH_PRECISION)?;

        // Skip if we've already seen this geohash
        if seen_geohashes.contains(&geohash_str) {
            continue;
        }

        // Add to seen set and write to output
        seen_geohashes.insert(geohash_str.clone());

        // Write the geohash, latitude, and longitude to the output CSV.
        writer.serialize(OutputRecord {
            geohash: &geohash_str,
            latitude,
            longitude,
        })?;
    }

    println!("Found {} unique geohashes", seen_geohashes.len());

    // Ensure all data is written to the file.
    writer.flush()?;

    println!("✅ Successfully created '{}' with geohashes.", output_path);

    Ok(())
}

/// Find the column indices for latitude and longitude by checking common header names.
fn find_lat_lon_columns(headers: &csv::StringRecord) -> Result<(usize, usize), Box<dyn Error>> {
    let lat_names = ["latitude", "lat", "y"];
    let lon_names = ["longitude", "lon", "lng", "long", "x"];

    let mut lat_idx: Option<usize> = None;
    let mut lon_idx: Option<usize> = None;

    for (i, header) in headers.iter().enumerate() {
        let lower = header.to_lowercase();
        if lat_names.contains(&lower.as_str()) {
            lat_idx = Some(i);
        }
        if lon_names.contains(&lower.as_str()) {
            lon_idx = Some(i);
        }
    }

    match (lat_idx, lon_idx) {
        (Some(lat), Some(lon)) => Ok((lat, lon)),
        (None, _) => {
            Err("Could not find latitude column. Expected: LATITUDE, Lat, lat, or y".into())
        }
        (_, None) => Err(
            "Could not find longitude column. Expected: LONGITUDE, Lon, lon, lng, long, or x"
                .into(),
        ),
    }
}
