


# STEPS TO BUILD AND GENERATE THE FIRMWARE BIN FILES

1. build the project with the following command:

   ```bash
   cargo build --release
   ```
2. create the firmware.bin files with the following command:

   ```
   espflash save-image --chip esp32c3 \
  ./target/riscv32imc-unknown-none-elf/release/gps \
  firmware.bin
   ```