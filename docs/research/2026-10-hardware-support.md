# Supporting any hardware: benchmark

- **Date:** 2026-10-23
- **Status:** Research report, input to the [hardware support spec](../specs/hardware-support.md) and ADR 0013.

Date: 2026-10-23. Research only; nothing in the repositories was changed.

Question: how do mature systems keep hardware specifics subordinate to the OS (data plus adapters, never hard-coded), so one OS runs on any SoC, board, DAC, display, faceplate, HAT or USB peripheral, and disables what is missing (SYS-040)?

Method: primary sources fetched by direct URL on 2026-10-23 (listed per section). Anything not confirmed from a fetched source is marked **UNVERIFIED**. No numbers were invented; the only numbers quoted come from the fetched sources.

Already in the repositories (read, not changed): `public/system/docs/adr/0011-package-layers.md` (L0 to L7) and `public/system/docs/specs/os-architecture.md` §4.3 to §4.4 (`dap-input`, `dap-hatd`, `org.opendap.device.Capabilities` with `present` / `absent` / `disabled: reason`, context keys such as `device.output.balanced`). The proposal at the end builds on those and does not replace them.

---

## 1. Android Treble

What it does

- **Vendor/system split with a machine-checked contract (VINTF).** "A manifest describes what is provided and a compatibility matrix describes what is required." There are four documents: device manifest plus framework compatibility matrix, and framework manifest plus device compatibility matrix. They are reconciled during OTA, and a queryable API serves both static XML and runtime-collected data to OTA servers and CTS. The device declares a Target FCM Version, which tracks HAL deprecation.
- **AIDL HALs.** Interfaces that cross partitions are `@VintfStability` with `stability: "vintf"`. Servers are declared in the manifest as `<hal format="aidl">`, and VTS checks that each declared interface is frozen at a released version. Matrix entries carry a name, a version range, an interface and an instance. The matrix `optional` attribute "is deprecated after Android 15 and no longer has any effect". Matrices can also require kernel configs and sepolicy versions.
- **Device tree overlays.** "A bootloader using DTO can maintain the system-on-chip (SoC) DT and dynamically overlay a device-specific DT." The SoC vendor owns the base DTB; the ODM ships overlays in a `dtbo` partition.
- **GKI.** A generic core kernel plus vendor modules behind the KMI symbol lists, in `vendor_boot`, `vendor_dlkm` and `odm`.
- **GSI.** "Pure Android… that any Android device running Android 9 or higher can run successfully." Running CTS/VTS on a GSI proves that the vendor side implements its interfaces.
- **Framework adaptation.** Devices declare system features in XML, for example `<feature name="android.hardware.audio.output" />` ("ONLY devices that meet the CDD's requirements may declare this feature"). Apps query them with `PackageManager.hasSystemFeature()` and declare `<uses-feature android:required="true|false">`; Play filters on them. Aconfig flags are a separate mechanism that gates *software* rollout. They are not hardware detection.

**Lesson.** Draw an explicit, versioned contract between "the OS" and "the hardware support", check it by machine (on update and in tests), and prove it with a generic image on real hardware. Keep three things apart: *hardware features* (declared by the device, queried by apps), *interfaces* (HALs with frozen versions) and *software flags* (rollout). Android dropping `optional` is a warning: optionality belongs to the client's feature logic, not to the matrix.

Sources: source.android.com/docs/core/architecture/vintf · …/vintf/comp-matrices · …/aidl/aidl-hals · …/architecture/dto · …/kernel/modular-kernels · …/tests/vts/gsi · developer.android.com/guide/topics/manifest/uses-feature-element · android.googlesource.com/platform/frameworks/native/+/refs/heads/main/data/etc/android.hardware.audio.output.xml · source.android.com/docs/setup/build/feature-flagging/declare-flag

## 2. postmarketOS, Mobian, Phosh, Plasma Mobile

What it does

- **Packages per device.** In pmaports, "device packages … start with `device-`", next to kernel packages `linux-<vendor>-<codename>`, firmware packages, generic device packages and UI-specific subpackages (Phosh, Plasma).
- **`deviceinfo`** holds the per-device data, for example:
  - `deviceinfo_codename`, `_manufacturer`, `_name`, and `_chassis` (handset, tablet, laptop, watch…);
  - `deviceinfo_dtb`;
  - `deviceinfo_drm`, which replaced `gpu_accelerated` and "is used by pmbootstrap init to hide UIs which require DRM if the device lacks such support";
  - `deviceinfo_flash_method`, `deviceinfo_getty`, `deviceinfo_rootfs_image_sector_size`;
  - `deviceinfo_screen_width` and `_height`, needed only for touchscreen calibration.
- **Kernel-detectable data is retired from `deviceinfo`.** `deviceinfo_kernel_cmdline` is obsolete (now `kernel-cmdline.d` files). `deviceinfo_dev_keyboard` is obsolete because "systems now auto-detect keyboard presence".
- **Device categories** (main, community, testing, downstream, archived) carry explicit support criteria. Main requires Hardware CI, UEFI boot and generic kernels.
- **Naming.** docs.postmarketos.org and wiki.postmarketos.org now redirect (HTTP 301) to `nura.eco`, whose pages call the distribution "Nura". Whether this is an official rename is **UNVERIFIED**.
- **Phoc** (the Phosh compositor) configures outputs in `phoc.ini`, with per-output `mode` and `rotate` (including `flipped-*`). A `scale` key was not in the fetched example.
- **Mobian and Plasma Mobile.** Mobian's wiki moved to wiki.debian.org/Mobian. Its per-device packaging (kernels, tweaks packages) was not confirmed from a fetched source: **UNVERIFIED**. Plasma Mobile's device adaptation was not researched from primary sources: **UNVERIFIED**.

**Lesson.** One small data file per device, plus packages for kernel and firmware, is enough to support many devices, and the trend is to *delete* fields once the kernel or udev can detect them. Use the device description only for facts nothing else can supply. A support tier per device, with explicit criteria, keeps the support claims honest.

Sources: docs.nura.eco/pmaports/main/packaging/device-packages.html · …/device-packages/deviceinfo-reference.html · …/device-categorization.html · gitlab.gnome.org/World/Phosh/phoc (README, src/phoc.ini.example)

## 3. Linux devicetree, ACPI, EBBR, flash-kernel

What it does

- **What DT is for.** DT is "simply a data structure that describes the hardware", so that "board and device support … become data driven". The root `compatible` is "a sorted list of strings starting with the exact name of the machine", from most to least specific, with a caution against claiming compatibility between boards.
- **Bindings** are json-schema YAML, validated with `make dt_binding_check` (the schemas) and `make dtbs_check` (the DTS files). Bindings exist for:
  - panels: `panel-common.yaml` has `width-mm` and `height-mm` ("the physical area where images are displayed … rounded to the closest unit"), `rotation`, `flip-horizontal`, `flip-vertical` and `label`;
  - buttons: `gpio-keys.yaml` has `linux,code`, `label`, `linux,input-type`, `wakeup-source` and `debounce-interval` (default 5 ms);
  - audio: `simple-audio-card` has `widgets`, `routing` (sink and source pairs), `hp-det-gpios`, `name` and `dai-link`.
- **What DT has no binding for.** No generic audio-jack connector binding was found: `usb-connector.yaml` covers only USB connectors. No binding for button *positions* was found.
- **Overlays.** In the kernel, `of_overlay_fdt_apply()`, `of_overlay_remove()` and notifiers. The configfs interface (`/config/device-tree/overlays/<name>/{path,dtbo}`) is documented in the Raspberry Pi kernel tree. That it is absent from mainline is **UNVERIFIED** (the mainline overlay-notes page does not mention it). Any pointer kept into overlay nodes after removal "is considered to be a bug".
- **ACPI vs DT on arm64.** ACPI suits BSA/BBR servers ("AML allows the platform to encode hardware behavior, while DT explicitly does not"); DT remains suited to "vertically integrated devices".
- **EBBR v2.4.0** (2026-03-11) is a UEFI subset for embedded boards. Platforms provide DT or ACPI, "one, but not both", and the firmware supplies the DTB. An EBBR platform boots "an EBBR compliant operating system without any custom tailoring". Every BBR system is EBBR-compliant; the converse is not true.
- **Debian flash-kernel.** A machine database matches `/proc/device-tree/model` (`Machine:`) to a `DTB-Id`, a `Boot-Script-Path` and `Kernel-Flavors`. The DTB is copied from the `linux-image` package, or from `/etc/flash-kernel/dtbs` as a local override.

**Lesson.** DT plus mainline bindings is the right description of *electrical* hardware for the T1 class of device, and it is validated by machine. DT says nothing on purpose about behaviour, policy, physical layout or enclosure, so Blackbird OS needs a second description for those facts. Overlays are how a base SoC or board tree gains faceplate or HAT nodes. The DTB comes from the kernel package or the firmware (the EBBR model), keyed by `compatible` or `model`, never chosen by the OS image itself.

Sources: kernel.org/doc/html/latest/devicetree/usage-model.html · …/devicetree/overlay-notes.html · …/devicetree/bindings/writing-schema.html · kernel.org/doc/Documentation/devicetree/bindings/{display/panel/panel-common,input/gpio-keys,sound/simple-card,connector/usb-connector}.yaml · raw.githubusercontent.com/raspberrypi/linux/rpi-6.6.y/Documentation/devicetree/configfs-overlays.txt · kernel.org/doc/html/latest/arch/arm64/arm-acpi.html · arm-software.github.io/ebbr · salsa.debian.org/installer-team/flash-kernel (README)

## 4. Hardware description in Linux userspace

What it does

- **systemd hwdb.** Glob match lines followed by key-value lines, kept in `/usr/lib/udev/hwdb.d` (vendor) and `/etc/udev/hwdb.d` (local) and compiled to `hwdb.bin`; udev adds the matching properties to devices. For example, `70-av-production.hwdb` matches `usb:v0FD9p0060*` and sets `ID_AV_PRODUCTION_CONTROLLER=1`.
- **Sensor orientation in hwdb.** `60-sensor.hwdb` defines `ACCEL_MOUNT_MATRIX`, so that the "x and y axis matches the x and y axis of the display", and `ACCEL_LOCATION` (`base` or `display`). Its match keys are modalias plus DMI. No devicetree match key was found in that file.
- **ALSA UCM2.** Configs are found by "the ALSA driver name with the kernel driver name as fallback", under `conf.d/<driver>/…`, with virtual cards in `conf.virt.d`.
  - Standard device names: Speaker, Headphones, Headset, Line, Mic, SPDIF, HDMI, USB, Direct and others.
  - Values: `PlaybackPCM`, `PlaybackVolume`, `PlaybackSwitch`, `PlaybackMixerElem`, `JackControl`, `JackDev`, and `Priority` (1 to 10000).
  - Submissions must include `alsa-info.sh` output, and configs are checked by a UCM validator that emulates the hardware.
- **libinput quirks.** `.quirks` ini files with `MatchName`, `MatchBus`, `MatchVendor`/`Product`, `MatchDMIModalias`, `MatchDeviceTree` and `MatchUdevType`. "Model quirks are internal API and may change at any time."
- **Display size and scale.** DRM exposes `width_mm` and `height_mm` in `drm_display_info` (from the EDID or the panel), the panel orientation property ("how the lcd-panel is mounted inside the casing") and `non-desktop`. Mutter's `calculate_scale()`:
  - returns 1.0 when either dimension is 0;
  - computes the diagonal in inches;
  - targets 135 dpi below 20 inches (`UI_SCALE_MOBILE_TARGET_DPI`) and 110 dpi otherwise;
  - picks the nearest supported scale to `physical_dpi / target_dpi`.
- **Not fetched.** iio-sensor-proxy (its README was blocked by Anubis) and ModemManager plugins (HTTP 403) could not be read. Their usual descriptions are left out here: **UNVERIFIED**.

**Lesson.** Userspace already has *per-subsystem* description formats, keyed by kernel identity (modalias, DMI, DT compatible, ALSA driver name): hwdb for device properties, UCM2 for audio routing, volume and jacks, and libinput quirks for input. Blackbird OS should ship its board and component data in *these* formats wherever they apply, so that upstream tools work and the data can go upstream. Its own description should be kept for what has no upstream home. Scaling must use physical millimetres and never assume a resolution; it needs a defined fallback when `width_mm` is 0, which Mutter treats as "no data, scale 1". Upstream marks some of these formats as internal (libinput quirks), so Blackbird OS should not build its stable contract on top of them.

Sources: freedesktop.org/software/systemd/man/latest/hwdb.html · github.com/systemd/systemd hwdb.d/{60-sensor,70-av-production}.hwdb · github.com/alsa-project/alsa-ucm-conf (README, ucm2/README.md) · alsa-lib include/use-case.h · wayland.freedesktop.org/libinput/doc/latest/device-quirks.html · kernel.org/doc/html/latest/gpu/drm-kms.html · github.com/GNOME/mutter src/backends/meta-monitor.c

## 5. ChromeOS unibuild, Zephyr, Fuchsia DFv2

What it does

- **ChromeOS `cros_config` (unibuild).** One image serves many models. The model is selected at runtime by identity (`frid`, `sku-id` from "board strapping pins configured during board manufacturing", `platform-name`, `custom-label-tag`), through an `identity.bin` table of contents stored in SquashFS.
  - `hardware-properties` includes `form-factor`, `display-type`, `recovery-input`, `has-touchscreen`, `has-side-volume-button`, `stylus-category`, `has-backlight`, `has-keyboard-backlight`, `psu-type` and sensor booleans.
  - Audio config points to a per-model `cras-config-dir`, an optional `ucm-suffix`, `speaker-amp` and `sound-card-init-conf`.
  - The same data generates C code for the EC build.
  - How ARM models are identified by DT compatible was not confirmed in the fetched README: **UNVERIFIED**.
- **Zephyr.** Hardware model v2 is Architecture → CPU core → SoC (optional series and family) → Board, with qualifiers and revisions; a board is "devicetree + Kconfig defconfig" described by `board.yml`.
  - **Shields** (add-on boards) live in `boards/shields/<shield>/` with `shield.yml`, `<shield>.overlay`, `Kconfig.shield` and `Kconfig.defconfig`, and are applied with `--shield`.
  - A shield works on any board because boards expose **connector nexus nodes** (`arduino_header`, `feather_header`, `mikrobus_header`, `pico_header`…) whose `gpio-map` translates connector pins to SoC pins.
- **Fuchsia DFv2.** Drivers declare **bind rules** in a bind language over node properties (string keys with bool, string, int or enum values, namespaced in bind libraries). Parent drivers add child nodes with `NodeProperty2` vectors. **Composite** drivers bind across several parents, and parents can be marked `optional`. The board-driver detail is thin in the fetched pages: **UNVERIFIED** beyond "drivers add child nodes with properties".

Lesson.

- From ChromeOS: identity, then a model record, then typed properties, from one image. The same record feeds every consumer, including build-time generators.
- From Zephyr: a *connector abstraction* on the board is what makes add-ons portable. The HAT and faceplate connectors should be named nexus-like nodes in the DT, so a faceplate or HAT overlay never names SoC pins.
- From Fuchsia: matching is declarative and data-driven (properties plus rules), not code. Composites with optional parents fit "a DAC + amp + jack-detect" component.

Sources: chromium.googlesource.com/chromiumos/platform2/+/HEAD/chromeos-config/README.md · docs.zephyrproject.org/latest/hardware/porting/{shields,board_porting}.html · fuchsia.dev/fuchsia-src/development/drivers/tutorials/bind-rules-tutorial · fuchsia.dev/…/device_driver_model/driver-binding

## 6. Rockbox and Raspberry Pi HATs

What they do

- **Rockbox** hard-codes each target at compile time. For example, `firmware/export/config/ipodvideo.h` has:
  - `MODEL_NAME "Apple iPod Video"`;
  - `LCD_WIDTH 320`, `LCD_HEIGHT 240`, `LCD_DEPTH 16`;
  - `CONFIG_KEYPAD IPOD_4G_PAD`, `HAVE_WM8758`, `HAVE_SCROLLWHEEL`, `HAVE_RECORDING`;
  - `INPUT_SRC_CAPS (SRC_CAP_LINEIN | SRC_CAP_FMRADIO)`.

  Buttons reach the UI through per-target keymaps, `apps/keymaps/keymap-<target>.c`, which hold per-context tables such as `{ ACTION_STD_NEXT, BUTTON_SCROLL_FWD, BUTTON_NONE }`. The good part is the indirection from *semantic actions* to buttons. The cost is one binary per target, and a feature without its `HAVE_*` macro is compiled out, not disabled.
- **Raspberry Pi HAT EEPROM.** A header followed by CRC-16-checked atoms: `0x0001` vendor info (RFC 4122 UUID, product ID, version, vendor and product strings), `0x0002` and `0x0005` GPIO bank maps, `0x0003` a Linux DT blob, `0x0004` manufacturer custom data. The firmware reads it at boot and applies the overlay, "including loading all the necessary drivers".
  - HAT+ tooling (`eeptools`) supports stackable HAT+ boards at distinct I2C addresses and preserves the product UUID generated by `eepmake`.
  - Whether HAT+ replaces the embedded blob with an overlay *name* is **UNVERIFIED** (not in the fetched pages).
  - `config.txt` `dtoverlay` and `dtparam` are the manual path.

**Lesson.** Keep Rockbox's action-to-button indirection, but make it *data at runtime*, not `#ifdef`, so that one image serves every faceplate and a missing button is a disabled binding, not a missing build. Take the HAT precedent (an ID EEPROM carrying a UUID, product and version, and a link to the description) for both HATs and faceplates. The EEPROM should identify the board and link to its signed description; the HAT standard already plans this ID EEPROM. Embedding the full DT blob in the EEPROM makes it impossible to fix the description in the field.

Sources: raw.githubusercontent.com/Rockbox/rockbox/master/firmware/export/config/ipodvideo.h · …/apps/keymaps/keymap-ipod.c · github.com/raspberrypi/hats (README.md, eeprom-format.md) · github.com/raspberrypi/utils eeptools/README.md · raspberrypi.com/documentation/computers/config_txt.html

## 7. Describing the physical UI to software

What they do

- **Android input.** Key layout files (`.kl`) are looked up in `/odm`, `/vendor`, `/system` and `/data/system/devices/keylayout/`, by USB vendor, product and version, then by device name, falling back to `Generic.kl`. Each line reads `key <linux code> <android keycode> [FUNCTION|GESTURE|VIRTUAL]`. They sit alongside key character maps and input device configuration files. These files describe *meaning*, not *position*.
- **Android display cutouts.** `DisplayCutout` gives apps safe insets and bounding rects, and apps choose `layoutInDisplayCutoutMode` (`default`, `shortEdges`, `never`, `always`). The OEM declares the geometry in `config_mainBuiltInDisplayCutout`, so the shape is device data consumed by a generic API.
- **Wear OS** pushes responsive layouts, a breakpoint on `screenWidthDp` (the example uses 225) and previews across devices. `Configuration.isScreenRound()` was not confirmed in the fetched pages: **UNVERIFIED**.
- **Garmin Connect IQ** publishes per-device resolution, screen shape (`round`, `rectangle`, `semi-octagon`), screen technology and API level. The button set was not on the fetched page: **UNVERIFIED**.
- **iOS safe areas.** The page did not render, so the details are **UNVERIFIED**.

**Lesson.** Successful platforms publish the *shape* of the screen (outline, cutouts, safe insets) and the *meaning* of keys as device data behind a stable app-facing API, and apps lay out responsively instead of per model. None of these sources publish *button positions* as data. Blackbird OS needs that because its UI points at physical buttons and faceplates vary. This is new ground, and should be designed as a small, versioned extension of the same idea: a faceplate geometry record in millimetres.

Sources: source.android.com/docs/core/interaction/input/key-layout-files · developer.android.com/develop/ui/views/layout/display-cutout · developer.android.com/training/wearables/compose/screen-size · developer.garmin.com/connect-iq/compatible-devices

---

## Implications for Blackbird OS (proposal, for the maintainers to decide)

Status: options grounded in the sources above. Nothing here is decided. Where it touches ADR 0011 and the architecture spec it builds on them; any change to them would need its own ADR or RFC.

### A. Layering

| Layer | Owns | Described by | Precedent |
|---|---|---|---|
| H0 SoC support | CPU, clocks, pinctrl, GPU, codecs-on-SoC, the boot chain | mainline SoC `.dtsi`, kernel config | Zephyr SoC, Android SoC DTB, EBBR |
| H1 Board | Power tree, PMIC, fuel gauge, DAC/amp wiring, radios, connectors (faceplate, HAT, jacks, USB) as **named connector nodes** | board `.dts` (top `compatible` most→least specific) + OpenDAP device description | Zephyr board + nexus nodes, flash-kernel `Machine` |
| H2 Components | One record per part: DAC, amp, panel, fuel gauge, TPM, NFC, haptics, with limits from the datasheet | DT binding properties + component record (limits, units, source) | Fuchsia composites, ChromeOS `hardware-properties` |
| H3 Faceplate | Display (or none), touch, buttons and their positions, LEDs, encoder | DT overlay against the faceplate connector + faceplate geometry record; ID EEPROM | Zephyr shield, HAT EEPROM, Android cutout config |
| H4 HATs | Whatever the HAT carries | DT overlay against the HAT connector + L5 bundle manifest `[hardware]`; ID EEPROM (HCS-017) | RPi HAT, Zephyr shield |
| H5 External peripherals | USB DACs, BT sinks, docks | Nothing pre-declared: USB descriptors, hwdb/UCM2 entries keyed by modalias | hwdb, UCM2 `conf.d/USB-Audio/…`, Android `.kl` by VID/PID |

Rule: every layer only refers to the connector names of the layer below it, never to SoC pins (Zephyr). A new SoC changes H0 and H1 only.

### B. Description format

1. **Devicetree for everything that has a binding**: buses, regulators, GPIOs, `panel-common` `width-mm`/`height-mm`/`rotation`, `gpio-keys` `linux,code`/`label`, `simple-audio-card` routing and `hp-det-gpios`. Validate with `dt_binding_check` and `dtbs_check`. New bindings go upstream, and are carried as patches until they land (OS-023).
2. **Upstream userspace formats where they exist**: UCM2 for routing, volume elements and jacks; hwdb for device properties and sensor mount matrices; libinput quirks only as a last resort, because upstream marks them as internal.
3. **An OpenDAP device description for what neither carries.** It is versioned, signed, and keyed by DT `compatible` or EEPROM product ID and version. It holds:
   - the physical layout: the outline of the enclosure and the faceplate in mm, and the display's active area, shape, corner radii and cutouts (as Android's `DisplayCutout`);
   - button positions, sizes and kinds, linked to their `linux,code` (DT gives the code, the description gives the place);
   - the display's physical size when the panel binding lacks it, or when the visible area differs from the panel's active area;
   - the output jacks (kind, balanced or single-ended, the detect line) linked to UCM2 device names;
   - the audio component limits (maximum output level, supported rates, formats and DSD modes, gain stages), each with its datasheet source, or marked **UNVERIFIED**;
   - safety limits (BRD-049), shown to the user.

   The schema is published (json-schema like DT bindings) and checked by a validator on the developer machine, in the spirit of UCM2's validator.

### C. Adapters

- One **component service** per component class (DAC, panel, buttons, fuel gauge, radio, haptics…). Each is a `driver` service started by udev or HAT attach (spec §4.3), implements a versioned `org.opendap.hw.<class>` Varlink interface, and is the only code that knows the part.
- Matching is declarative: a service declares the DT `compatible` strings, USB IDs or EEPROM IDs it serves (Fuchsia bind rules, Android `.kl` lookup order). The orchestrator binds services to hardware. Services never probe the hardware to guess the model.
- Semantic input (`dap-input`, `org.opendap.input.Actions`) maps Linux key codes to actions through data, never through code (the runtime version of Rockbox's action-to-button tables).

### D. Packaging (ADR 0011 layers)

| Item | Layer |
|---|---|
| Bootloader, firmware-provided DTB (EBBR) | L0 |
| Kernel, SoC and board DTBs and overlays, base hwdb/UCM2/quirks, first-party component services, the description schema and validator | L1 (built from L2 `.deb`s) |
| Board, faceplate and component descriptions for known hardware | L2 `.deb` inside L1, like pmaports `device-*` / `linux-*` / `firmware-*` |
| Kernel modules, firmware loaders or daemons a third-party board or HAT needs | L3 sysext |
| HAT drivers that the sandbox can run, the HAT description and overlay | L5 (an L4 bundle with `[hardware]`) |
| Descriptions for new faceplates and HATs, resolved from their EEPROM ID | L6 registry artifacts, signed |
| Owner overrides (as `/etc/flash-kernel/dtbs` and `/etc/udev/hwdb.d`) | L7, documented, never silent |

### E. Discovery

Discovery runs in order, each step adding to the previous one:

1. DT root `compatible` (or `model`) selects the board and its description, the flash-kernel pattern.
2. The faceplate EEPROM, then each HAT EEPROM: an RPi-style vendor-info atom (UUID, product ID, product version) points to a description and an overlay, which are applied before the UI starts (faceplate) or on attach (HAT).
3. udev, hwdb and modalias cover buses and USB, with USB Audio Class descriptors for USB DACs. The descriptor fields Blackbird OS would read are **UNVERIFIED** here because the USB-IF specification was not fetched.
4. Service probes report what the hardware actually answers, such as rates and formats from ALSA.

The orchestrator merges these steps into one record. A description that contradicts a probe is reported, never trusted silently.

### F. Capability publication to the UI

- `org.opendap.device.Capabilities` (spec §4.4) stays the single source for the UI and for peers, with typed values such as `display.shape`, `display.size_mm`, `display.safe_insets`, `buttons[]` with positions, `outputs[]` and `audio.limits`, and states `present`, `absent` or `disabled: reason`.
- UI layout is responsive to physical size and shape (Mutter's dpi targets, Wear OS breakpoints), with defined fallbacks when the size is unknown, and supports "no display" faceplates through a non-visual UI path.
- Apps and capabilities declare needs as `when` predicates and `uses-feature`-style `required` / `optional` in their manifests. The registry can filter on them (as Play does), and a missing optional feature disables only that feature.

### G. Compatibility and versioning (VINTF-like)

- **Provided.** Each board, faceplate and HAT description declares the `org.opendap.hw.*` interfaces and versions it provides.
- **Required.** Each Blackbird OS release publishes a matrix of the interface version ranges, kernel configs and binding versions it requires.
- **When it is checked.** On a RAUC update before install (Treble checks at OTA), at faceplate or HAT attach, and in tests.
- **Freezing.** Interfaces are frozen at each release (AIDL), and changes only add.
- **Optionality.** It lives in the consumer's feature logic, not in the matrix; Android dropped `optional` after 15.

### H. Testing

- **A virtual device**: QEMU with a generated DT plus fake component services implementing `org.opendap.hw.*`, run against the same description files. This plays the GSI role: the generic OS passes the same tests on virtual and real hardware.
- **Description fixtures**: boards with no display, a round OLED, a 1-bit OLED, several DACs, a USB DAC only, HATs attached and detached at runtime. Every fixture runs through the validator and the UI snapshot tests.
- **Captured real hardware**, as UCM2's `alsa-info.sh` emulation does.
- **Support tiers** per board or faceplate with explicit criteria, as pmaports categories do.

### I. Never hard-code

- **Hardware identity**: SoC, board or part names and pin numbers outside H0/H1 sources and component services.
- **Display**: resolution, density, shape, orientation or the presence of a display.
- **Buttons**: count, names, positions or Linux codes in the UI or in apps; actions only.
- **Audio**: DAC, amp or codec names, mixer control names (UCM2 holds them), rates, formats or output levels; audio limits come from the description and the probes.
- **Outputs**: which jacks exist, or that a jack has a detect line.
- **Optional parts**: the presence of the fuel gauge, PMIC, radios, TPM, NFC or haptics; all are capabilities.
- **HATs and faceplates**: HAT or faceplate product IDs anywhere except their descriptions.
- **Feature removal**: no build-time `#ifdef`/feature removal per target (the Rockbox anti-pattern); disable at runtime with a reason (SYS-040).

### Open items, still UNVERIFIED

- Configfs overlays in mainline.
- HAT+ overlay-by-name.
- cros_config identity by DT on ARM.
- iio-sensor-proxy and ModemManager internals.
- Plasma Mobile and Mobian device mechanics.
- iOS safe-area computation.
- Wear OS `isScreenRound`.
- Connect IQ button metadata.
- USB Audio Class descriptor fields.
- The postmarketOS to "Nura" rename.
