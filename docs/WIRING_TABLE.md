# Wiring reference

Pin numbers below are the **ESP32-C3 Super Mini** wiring that the firmware in
`firmware/src/bin/main.rs` targets. An earlier revision ran on a full devkit with
the GPS on GPIO4/GPIO5 and the button on GPIO9; if you are on that board, change
the pins in `init_gps_mode` and the `mode_button_pin!` macro.

## Connection table

### GPS module (UART, 9600 baud)

| GPS pin | Wire colour (suggestion) | ESP32-C3 pin | Function |
|---|---|---|---|
| VCC | Red | 5V or 3.3V | Power — check your module's datasheet |
| GND | Black | GND | Ground |
| TX | Yellow | **GPIO21** | GPS transmit → ESP receive |
| RX | Green | **GPIO20** | GPS receive ← ESP transmit |

### WS2812B LED strip (3 LEDs)

| LED pin | Wire colour | ESP32-C3 pin | Function |
|---|---|---|---|
| VCC / 5V | Red | 5V | Power — 5V required |
| GND | Black | GND | Ground |
| DIN | Blue/white | **GPIO8** | Data, driven over RMT |

The firmware writes three pixels at a time and sizes its RMT buffer at 75 words
(24 bits × 3 LEDs + reset). Driving a different number of LEDs means changing
both the buffer length and the `[color; 3]` arrays in `firmware/src/gps.rs`.

### Buzzer

| Buzzer pin | Wire colour | ESP32-C3 pin | Function |
|---|---|---|---|
| + | Red | **GPIO2** | Active-high control |
| − | Black | GND | Ground |

### Mode button

| Button pin | ESP32-C3 pin | Function |
|---|---|---|
| One leg | **GPIO10** | Active-low, internal pull-up enabled in firmware |
| Other leg | GND | Ground |

Long-press (2 s) toggles between GPS mode and WiFi/OTA mode. The same pin is read
in both modes.

## Power distribution

```
USB 5V
  ├─> ESP32-C3 (via USB port)
  ├─> GPS module VCC   (or 3.3V, per module)
  ├─> WS2812B VCC      (5V required)
  └─> common GND       (all components share one ground)
```

## Pre-power checklist

- [ ] GPS TX → GPIO21, GPS RX → GPIO20 (swapped is the most common mistake)
- [ ] GPS VCC at the voltage its datasheet asks for
- [ ] LED DIN → GPIO8, LED VCC → 5V
- [ ] Buzzer + → GPIO2
- [ ] Button between GPIO10 and GND
- [ ] Every ground tied to one point

## Wire gauge

| Connection | Gauge | Length | Note |
|---|---|---|---|
| Power (5V, GND) | 22–24 AWG | short | Minimise voltage drop |
| GPS UART | 26–28 AWG | < 1 m | Signal integrity |
| LED data | 24–26 AWG | < 15 cm | Minimise capacitance |
| Buzzer | 24–26 AWG | any | Low current |

## Common mistakes

| Wrong | Right |
|---|---|
| GPS TX → GPIO20 | GPS TX → **GPIO21** (ESP receive) |
| LED powered from 3.3V | LED powered from **5V** |
| Separate grounds per module | One common ground |
| LED data run > 30 cm | Keep under 15 cm |

## Voltage reference

| Rail | Voltage | Max current |
|---|---|---|
| USB input | 5V | depends on source |
| ESP32-C3 3.3V | 3.3V | ~500 mA |
| GPIO | 3.3V logic | 40 mA per pin |
