use geohash::{encode, Coord};
use serde::Deserialize;
use std::error::Error;

// A struct to represent the columns we care about from the input CSV.
// Using `serde`'s rename attribute to match the CSV header names.
#[derive(Debug, Deserialize)]
struct InputRecord {
    #[serde(rename = "LATITUDE")]
    latitude: f64,
    #[serde(rename = "LONGITUDE")]
    longitude: f64,
}

// A struct for writing the clean, processed data to our output CSV.
#[derive(Debug, serde::Serialize)]
struct OutputRecord<'a> {
    geohash: &'a str,
    latitude: f64,
    longitude: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Define the input and output file paths.
    let input_path = "Chicago_Speed_Camera_Locations_20250911.csv";
    let output_path = "geodata.csv";

    // Create a CSV reader for the input file.
    let mut reader = csv::Reader::from_path(input_path)?;

    // Create a CSV writer for the new output file.
    let mut writer = csv::Writer::from_path(output_path)?;

    // Define the precision for the geohash. 7 is a moderate precision,
    // suitable for neighborhood-level locations.
    const GEOHASH_PRECISION: usize = 7;

    println!("Processing CSV file: '{}'", input_path);

    // Write the header for the output file, matching the format needed for step 2.
    writer.write_record(&["geohash", "latitude", "longitude"])?;

    // Iterate over each record in the input CSV.
    for result in reader.deserialize() {
        // Deserialize the row into our `InputRecord` struct.
        let record: InputRecord = result?;

        // Create a `Coord` struct required by the geohash crate.
        let coord = Coord {
            x: record.longitude, // `x` corresponds to longitude
            y: record.latitude,  // `y` corresponds to latitude
        };

        // Encode the coordinate to a geohash string.
        // The `?` will propagate any errors that might occur during encoding.
        let geohash_str = encode(coord, GEOHASH_PRECISION)?;

        // Write the geohash, latitude, and longitude to the output CSV.
        writer.serialize(OutputRecord {
            geohash: &geohash_str,
            latitude: record.latitude,
            longitude: record.longitude,
        })?;
    }

    // Ensure all data is written to the file.
    writer.flush()?;

    println!("✅ Successfully created '{}' with geohashes.", output_path);

    Ok(())
}
