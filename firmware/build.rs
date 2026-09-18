use std::env;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Camera coordinates, as produced by `tools/geohash-prepper`.
const GEODATA_CSV: &str = "data/geodata.csv";

fn main() {
    // Without these, editing the CSV or changing the AP credentials leaves the
    // previously generated table and the previously baked-in strings in place.
    println!("cargo:rerun-if-changed={GEODATA_CSV}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SPEEDME_AP_SSID");
    println!("cargo:rerun-if-env-changed=SPEEDME_AP_PASSWORD");

    // 1. Set the path for the generated code.
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("geodata.rs");
    let mut file = BufWriter::new(fs::File::create(&path).unwrap());

    // 2. Start building the phf_codegen::Map.
    let mut map_builder = phf_codegen::Map::new();

    // 3. Read the CSV and add entries to the builder.
    let data_str = fs::read_to_string(GEODATA_CSV)
        .unwrap_or_else(|e| panic!("unable to read {GEODATA_CSV}: {e}"));
    for line in data_str.lines().skip(1) {
        // Skip the header row
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() != 3 {
            continue;
        } // Skip malformed lines

        let geohash = parts[0];
        if geohash == "geohash" {
            continue;
        }

        if let (Ok(latitude), Ok(longitude)) = (parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
            // The key is the geohash string.
            // The value is a string representation of the Coordinates struct constructor.
            let value_str = format!(
                "Coordinates {{ latitude: {}, longitude: {} }}",
                latitude, longitude
            );

            map_builder.entry(geohash, &value_str);
        }
    }

    // 4. Finalize the map string and write it to the file.
    writeln!(
        &mut file,
        "static GEO_MAP: phf::Map<&'static str, Coordinates> = {};",
        map_builder.build()
    )
    .unwrap();
    // End of the compile-time geohash table generation.
    linker_be_nice();
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!("💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`");
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                "esp_wifi_preempt_enable"
                | "esp_wifi_preempt_yield_task"
                | "esp_wifi_preempt_task_create" => {
                    eprintln!();
                    eprintln!("💡 `esp-wifi` has no scheduler enabled. Make sure you have the `builtin-scheduler` feature enabled, or that you provide an external scheduler.");
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!("💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests");
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
