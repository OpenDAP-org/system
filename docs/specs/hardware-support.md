# Hardware support

- **Status:** Draft, 2026-10-23. Decided by Claude under the maintainers' delegation, from their rules:
  - the T1 is the reference hardware, but Blackbird OS is not limited to it;
  - everything hardware-specific is subordinate to the OS and never hard-coded (SYS-040, SYS-067, SYS-068).

  Recorded in [ADR 0013](../adr/0013-hardware-support-layers.md).
- **Benchmark:** [supporting any hardware](../research/2026-10-hardware-support.md): Android Treble and VINTF, postmarketOS, devicetree, Zephyr shields, ChromeOS unibuild, Fuchsia drivers, UCM2, hwdb, Rockbox.
- **Builds on:**
  - the [system architecture spec](os-architecture.md): §3 package layers, §4.3 services, §4.4 device capabilities;
  - ADR 0009 to ADR 0011;
  - the [shell and UI runtime spec](shell-and-ui-runtime.md);
  - the [faceplate interface](https://github.com/opendap-project/t1/blob/main/docs/specs/faceplate-interface.md) (HID identification, T1-020);
  - the HAT standard (HCS-017).

## 1. Summary

1. **Blackbird OS runs on hardware it describes, not on hardware it assumes.** Any combination of SoC, board, components, faceplate, HATs and external peripherals is supported. Each is described by data and served by adapters, and the T1 is the first, reference description.
2. **Six hardware layers** (H0 to H5) each refer only to the named connectors of the layer below. A new SoC changes two layers; a new faceplate or HAT changes one.
3. **Two descriptions, one merged record.**
   - Devicetree carries what the kernel needs, with upstream userspace formats (UCM2, hwdb) where they exist.
   - An **OpenDAP device description** carries what neither does: physical layout, button positions, display shape, output jacks, audio and safety limits with their sources.
   - The orchestrator merges both with what the hardware answers, into the `org.opendap.device.Capabilities` record every surface reads.
4. **One adapter per component class.** A driver service implements a versioned `org.opendap.hw.<class>` interface, bound to the hardware by declared matching rules. It is the only code that knows the part.
5. **The UI is designed for any hardware and optimised for each.** The design system defines its values in physical and angular units and converts them for the display it finds. Layouts, input profiles and the drawing of physical controls follow the description. The T1 is the first fully optimised case, not the rule (§6).

## 2. Layers

| Layer | Owns | Described by | Identified by |
|---|---|---|---|
| **H0 SoC** | CPU, GPU, display controller, I2S, clocks, the boot chain | Mainline SoC devicetree, kernel configuration | The devicetree's SoC compatible |
| **H1 Board** | Power tree, PMIC, fuel gauge, the audio chain's wiring, radios, and the **named connectors** (faceplate, HAT, jacks, USB) | Board devicetree + the board's device description | Root `compatible` and `model` |
| **H2 Components** | Each part on the board: DAC, amplifier, panel, fuel gauge, TPM, NFC, haptics, LEDs | Devicetree binding properties + a component record with its limits, units and datasheet sources | Devicetree compatibles |
| **H3 Faceplate** | Display (or none), touch, buttons and their positions, the status LED, encoders | Devicetree overlay against the faceplate connector + the faceplate's description | HID vendor, product and version (T1-020) |
| **H4 HATs** | Whatever the HAT carries | Overlay against the HAT connector + its L5 bundle's `[hardware]` table | The HAT EEPROM's identity (HCS-017) |
| **H5 External** | USB DACs, Bluetooth outputs, docks | Nothing declared in advance: USB descriptors, hwdb and UCM2 entries by modalias, and probes | USB and Bluetooth identities |

**Connector rule.** Every layer refers only to the connector names of the layer below, never to SoC pins, in the manner of Zephyr's shields. A faceplate or HAT written for the T1's connectors works on any board that exposes the same connectors.

## 3. The device description

A signed, versioned document, `opendap.device-description/v1` (TOML), with one per board, component, faceplate and HAT, keyed by its identity (§2). It holds only what devicetree and the userspace formats do not.

| Section | Content |
|---|---|
| `identity` | Kind (board, component, faceplate, HAT), identity keys, version, maker |
| `layout` | Enclosure and module outlines in mm; the display's visible area, shape (rectangle, rounded, round), corner radii, cutouts, and its position on the front |
| `display` | Physical size when the panel binding lacks it or the visible area differs; refresh; technology (LCD, OLED, e-paper, monochrome); typical reading distance class (handheld, desk) |
| `controls` | Each control: kind (button, rocker half, dial, slider, switch), the Linux code its driver produces, its **semantic role** (`action`, `volume-up`, `volume-down`, `main`, `back`, `select`, `menu`…), position (edge, offset from a reference corner in mm, size), shape and tactile marks |
| `outputs` | Each jack and output: kind (3.5 mm, 4.4 mm balanced, line, optical, speaker), detection, the UCM2 device it maps to |
| `audio` | Per component: rates, formats, DSD modes, filters (named from the datasheet), volume range and step, gain stages, maximum output level, each with its source or **UNVERIFIED** |
| `power` | Battery chemistry and capacity source, charging limits |
| `safety` | Limits the hardware imposes and why (BRD-049), shown to the user |
| `provides` | The `org.opendap.hw.*` interfaces and versions this hardware is expected to provide (§7) |

- **Validation:** the schema is published and checked by a validator on the developer machine and in the image build, as devicetree bindings and UCM2 configurations are checked.
- **Fixes after release:** descriptions are L2 packages for known hardware and L6 registry artifacts for new faceplates and HATs (§5), so a description can be corrected without a new board.
- **Owner overrides:** live in L7 and are documented and visible, never silent.

## 4. Adapters

- **One component service per class**, a `driver` service (system architecture spec §4.2), implementing a versioned interface:

  | Class | Interface | Reference implementation on the T1 |
  |---|---|---|
  | DAC and output stage | `org.opendap.hw.audio.Dac`, `.Output` | CS43198 and the board's output stage |
  | Display | `org.opendap.hw.Display` | The faceplate's panel through DRM |
  | Controls | `org.opendap.hw.Controls` | `gpio-keys` and the faceplate's HID |
  | Battery and charging | `org.opendap.hw.Battery` | Fuel gauge and charger |
  | Status LED | `org.opendap.hw.Led` | The faceplate's RGB LED driver |
  | Haptics | `org.opendap.hw.Haptics` | DRV2605L |
  | Key holder | `org.opendap.hw.KeyHolder` (behind `dap-keystore`) | TPM 2.0 |
  | NFC | `org.opendap.hw.Nfc` | The NFC tag, and the reader where fitted |
  | Radios | Standard services (iwd, BlueZ) | The Wi-Fi and Bluetooth module |

- **Declarative matching.** Each service declares the devicetree compatibles, USB identities or HAT and faceplate identities it serves, and the orchestrator binds it, in the manner of Fuchsia's bind rules. No service probes to guess a model.
- **Semantic input.** `dap-input` maps Linux codes to the roles of the description's `controls`, through data only (the runtime form of Rockbox's action tables). The UI and capabilities see roles, never keys.
- **Generic fallbacks.** A component without its own adapter still works through the generic one for its class: ALSA with UCM2 for audio, DRM for display, evdev for controls. Its features are then limited to what the generic path can see, and say so (SYS-040).

## 5. Packaging

Mapped onto the package layers (ADR 0011):

| What | Layer |
|---|---|
| Bootloader, firmware-provided devicetree where the platform follows EBBR | L0 |
| Kernel, SoC and board devicetrees and overlays, base hwdb, UCM2 and quirks, first-party component services, the description schema and validator | L1, built from L2 packages |
| **Board support packages**: one per board, carrying its devicetree, descriptions and tuned packages (OS-004, OS-023), in the manner of postmarketOS's device, kernel and firmware packages | L2, composed into that board's image |
| Kernel modules, firmware loaders or daemons a third-party board or HAT needs | L3 system extensions |
| HAT drivers the sandbox can run, with the HAT's description and overlay | L5 |
| Descriptions for new faceplates and HATs, resolved from their identity | L6, signed |
| Owner overrides | L7 |

One image per board, one OS for all.

- Each image is the common Blackbird OS plus one board support package, trimmed and tuned for that board (OS-004).
- The OS itself contains no board's characteristics.
- An image may also carry several boards' packages and select one at boot from the devicetree, as ChromeOS selects a model.

## 6. What the UI receives

`org.opendap.device.Capabilities` (system architecture spec §4.4) publishes the merged record:

- `display`: size in mm, pixels, shape, safe insets, technology, distance class;
- `controls`: roles, positions, shapes;
- `outputs`;
- `audio` limits;
- `haptics`, `nfc`, `led` and the rest, each `present`, `absent` or `disabled: reason`.

From it, the shell and the hosts:

- **scale the design system** from physical and angular units to the display's pixels (design system §18, DS-022). Where the size is unknown, they fall back to a scale of 1, as GNOME does;
- **choose the layout class:**
  - small portrait handheld (the T1);
  - small landscape;
  - round;
  - monochrome or character displays;
  - no display, with a non-visual path through voice guidance and the buttons;
- **choose the input profiles:** touch, focus, dial;
- **draw physical controls where they are.** The volume level sits beside the volume controls, wherever the description places them. Power choices sit beside the control whose role is `main`. When a role has no physical control, its function moves to the screen.

Each layout class is optimised for its characteristics, not scaled from another. The T1's class is the first one designed in full; the others follow the same principles with their own optimisation.

## 7. Compatibility

- **Provided:** each description declares the `org.opendap.hw.*` interfaces and versions its hardware provides.
- **Required:** each Blackbird OS release publishes the interface version ranges, kernel options and binding versions it requires.
- **When:**
  - checked on an L1 update before it installs (Android checks this at OTA time);
  - checked when a faceplate or HAT attaches;
  - checked in tests.

  An incompatible piece is reported with its reason, and the update or the attach does not proceed silently.
- **Interfaces only grow** within a major version and are frozen at each release.
- **Optionality** belongs to each consumer's feature logic, not to the contract (Android dropped `optional` from its matrices).

## 8. Testing

- **The virtual device** (OS-018) is a description too: QEMU with a generated devicetree and fake component services implementing `org.opendap.hw.*`, run against the same description files.
- **Fixtures** cover more than the T1:
  - a board with no display;
  - a round OLED;
  - a 1-bit monochrome display;
  - different DACs, or a USB DAC only;
  - no TPM, no NFC, no haptics;
  - landscape;
  - HATs attached and detached at runtime.

  Every fixture runs through the validator and the UI snapshot tests.
- **Support tiers** per board and faceplate, with stated criteria, so a community port knows what "supported" means.

## 9. Never hard-coded

Outside the H0 and H1 sources and the component services:

- SoC, board or part names, and pin numbers.
- Display resolution, density, shape, orientation, technology, or whether there is a display.
- Buttons: count, names, positions or Linux codes. The UI and capabilities use roles only.
- DAC, amplifier or codec names, mixer controls (UCM2 holds them), rates, formats or output levels.
- Which outputs exist, or whether an output detects insertion.
- Whether the fuel gauge, PMIC, radios, TPM, NFC, haptics or LEDs exist: all are capabilities.
- Faceplate and HAT identities, anywhere but their descriptions.

The T1's values appear in documents only as **the T1 reference board's** description, never as rules of the OS or of the design system.
