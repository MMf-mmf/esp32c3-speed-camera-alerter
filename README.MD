


# STEPS TO BUILD AND GENERATE THE FIRMWARE BIN FILES

1. build the project with the following command (the defmt_log=warn is to reduce the logging output for better performance when running in production to include logging in development use "DEFMT_LOG=debug"):

   ```bash
   DEFMT_LOG=off cargo build --release
   ```
2. create the firmware.bin files with the following command:

   ```
   espflash save-image --chip esp32c3 \
  ./target/riscv32imc-unknown-none-elf/release/gps \
  firmware.bin
   ```