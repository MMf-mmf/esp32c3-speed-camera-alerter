# Schematic

Pin assignments match `firmware/src/bin/main.rs` on the ESP32-C3 Super Mini.
See [WIRING_TABLE.md](WIRING_TABLE.md) for the quick-reference version.

## Bill of materials

| Component | Description | Qty |
|---|---|---|
| ESP32-C3 Super Mini | Main microcontroller (RISC-V, WiFi) | 1 |
| GPS module | UART NMEA output, 9600 baud | 1 |
| WS2812B LEDs | Addressable RGB, driven as a strip of 3 | 3 |
| Buzzer | Active (self-oscillating) | 1 |
| Momentary button | Mode toggle | 1 |
| USB cable | 5V power / flashing | 1 |

## Pin assignments

| GPIO | Connected to | Direction | Notes |
|---|---|---|---|
| GPIO20 | GPS RX | UART TX | ESP transmits to GPS |
| GPIO21 | GPS TX | UART RX | ESP receives NMEA sentences |
| GPIO8 | WS2812B DIN | Output (RMT) | 3-LED strip |
| GPIO2 | Buzzer + | Output | Active high |
| GPIO10 | Mode button | Input, pull-up | Active low, 2 s long-press |
| 5V | GPS VCC, LED VCC | Power | |
| GND | All modules | Ground | Single common point |

## Block diagram

```
                       ESP32-C3 Super Mini
                   ┌──────────────────────────┐
  USB 5V ──────────┤ USB                      │
                   │             GPIO20 (TX) ─┼────────> GPS RX
                   │             GPIO21 (RX) ─┼────────< GPS TX
                   │                          │
                   │             GPIO8 ───────┼────────> WS2812B DIN ─> LED ×3
                   │             GPIO2 ───────┼────────> Buzzer (+)
                   │                          │
                   │             GPIO10 ──────┼────────< Mode button ─> GND
                   │                          │
                   │             5V ──────────┼──┬─────> GPS VCC
                   │                          │  └─────> LED VCC
                   │             GND ─────────┼────────> common ground
                   └──────────────────────────┘
```

## Notes on the passive components

- **WS2812B** needs 5V. Its data line is tolerant of the ESP32-C3's 3.3V output in
  practice, but a level shifter is the correct fix if you see flicker.
- A **100 µF** electrolytic across the LED 5V/GND rail and a **330 Ω** resistor in
  series with the data line both help with stability; neither is required.
- An **active** buzzer works with direct GPIO control, which is what the firmware
  assumes. A passive buzzer needs PWM and will be silent as wired here.
- If the buzzer draws more than 20 mA, drive it through an NPN transistor
  (GPIO2 → 1 kΩ → base, collector → buzzer +, emitter → GND).
- Keep the **GPS antenna** clear of the LED strip and any switching supply.

## Indicator behaviour

| State | LED | Pattern |
|---|---|---|
| No GPS fix | Yellow, low brightness | 1 s on / 1 s off |
| GPS fix, no alert | Green, low brightness | 200 ms blink every 10 s |
| Camera ahead | Red, high brightness | Solid |
| — | Buzzer | Two 100 ms beeps, once per alert |

## Timing

| Parameter | Value |
|---|---|
| GPS UART | 9600 baud |
| RMT clock | 80 MHz |
| Proximity check | every 3 s |
| Alert distance | 0.244 km (≈800 ft) |
| Heading tolerance | ±25° |
| Minimum speed to alert | 10.433 knots (≈12 mph) |

## Bring-up sequence

1. Check connections against the table above.
2. Continuity-test the grounds.
3. Measure the 5V and 3.3V rails before connecting the modules.
4. Power on — the LED should blink yellow (no fix yet).
5. Watch the UART at 9600 baud for `$GPGGA` / `$GPRMC` sentences.
6. Once the module has sky view, the LED should switch to the green 10 s blink.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| No GPS data | TX/RX swapped | Swap GPIO20 and GPIO21 |
| Permanent yellow blink | No satellite fix | Move outdoors, wait for cold start |
| LED dark | Wrong voltage or pin | Confirm 5V rail and GPIO8 |
| LED colours wrong | Insufficient power | Add the 100 µF capacitor |
| Buzzer silent | Passive buzzer, or polarity | Use an active buzzer; check + / − |
| Board resets under load | Overcurrent | Better 5V supply, add capacitors |
