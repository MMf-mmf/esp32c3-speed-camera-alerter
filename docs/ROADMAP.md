# Roadmap

Where this is going. Done items are kept for context on what has already been
proven out on hardware.

## Done

- [x] Runs on the ESP32-C3 Super Mini (the project started on a full devkit).
- [x] Production-ready OTA update page served from the device.
- [x] WiFi/OTA mode and GPS mode share one button and one binary.
- [x] Logging switched to `defmt`, and compiled out for production builds with
      `DEFMT_LOG=off`.

## Firmware

- [ ] Persist the alert region in flash instead of compiling it in, so a new
      dataset does not need a new binary.
- [ ] Support more than one region in a single image.
- [ ] Recover gracefully from a GPS module that stops responding, rather than
      sitting on a stale fix.
- [ ] Measure and document current draw in each mode; the proximity check and
      the LED task both wake more often than they need to.

## Data pipeline

- [ ] Teach `geohash-prepper` to merge several input CSVs in one run.
- [ ] Validate coordinates against a bounding box for the target region and
      report rows that fall outside it.

## Hardware

- [ ] Solder the harness and fit it into the enclosure. The GPS, buzzer and LED
      strip each need 5V and ground, so the three power and three ground lines
      need a one-to-three splice.
- [ ] Cut the LED strip into a 3-LED segment and tin the pads.

### Custom PCB

Replace the breadboard harness with a board carrying the ESP32-C3, GPS module,
battery management and LED indicator.

- [ ] Schematic capture
- [ ] PCB layout
- [ ] Bill of materials
- [ ] Fabrication and assembly

Tooling options: [KiCad](https://www.kicad.org/) (open source, industry
standard), Fritzing (easiest, limited), EagleCAD. Fabrication: JLCPCB, PCBWay,
OSH Park.

### Enclosure

- [ ] Replace the three round cut-outs in the 3D-printed shell with four 5 mm
      square holes, 11 mm apart, accounting for the curvature of the shell.
