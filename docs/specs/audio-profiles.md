# Audio profiles

- **Status:** Draft, 2026-10-23. Decided by Claude under the maintainers' delegation, from the [benchmark](https://github.com/opendap-project/t1/blob/main/docs/research/ux/2026-10-audio-profiles.md); values are to be confirmed on the prototype.
- **Requirements:** T1-064 (audio profiles), SYS-041 (audio UX: audiophile first, engineer one step away), SYS-040 (software not limited by one hardware), OS-020 (bit-perfect, DSD), SYS-012 (hearing safety), T1-057 (experience budgets), T1-061 (Action long press).

An audio profile is a named listening setup: which processing runs, how the DAC and output are set, and which headphone it is for. Profiles sit on one axis, from the audiophile, who wants to enjoy music with sparkle and high quality, to the engineer, who wants to monitor, analyse, compare and catch the smallest detail (SYS-041). Defaults always serve the audiophile; everything the engineer needs is one step away. The screen always says which profile is playing and whether the signal is untouched.

## 1. Profiles

| Profile | For | Signal | Bit-perfect |
|---|---|---|---|
| **Direct** (default) | Listening: the music as recorded, at the best quality the hardware gives | Untouched: no processing in software. Volume in the DAC; DAC filter and output as set | Yes |
| **Calibrated** | Listening on a known headphone, tuned to a preference target (Harman, preferred by most listeners in Olive and Welti's tests) | The headphone corrected to the target, from an open measurement database (AutoEq) or the user's own measurement | No |
| **Personal** | Audiophile sessions with their own colour and texture | Any of the blocks in §2, in any order, with a name and an optional headphone | No, unless it only uses hardware settings |
| **Monitor** | Engineering: monitoring, analysis, comparison | Untouched, or corrected to a flat studio target; the engineering tools of §5 shown on Now Playing | Yes when untouched |

- **Default.** A new headphone starts on Direct: high quality and untouched, with nothing to set. When the headphone is recognised and a measurement exists, the T1 offers Calibrated in one tap; it never changes the signal on its own.
- **One step to the engineer.** Monitor is in the same profile list as the others; choosing it is one tap, and it brings its tools with it.
- Profiles that only change hardware settings (DAC filter, DAC volume, output) stay bit-perfect, and the screen shows it.

## 2. Building blocks

Every block is part of the software on every device. Each one is turned on when the hardware it needs is present and disabled, with the reason shown, when it is not (SYS-040).

| Block | What it sets | Needs | On the T1 today |
|---|---|---|---|
| DAC filter | The DAC's interpolation filter | A DAC with selectable filters | Available: the CS43198 has five responses (exact list **UNVERIFIED** until checked against DS1156F2); keeps the data path bit-perfect |
| Gain | Output gain levels | An output stage with switchable gain | Disabled: fixed gain of about 1.6 (board ADR 0012) |
| Volume | In the DAC; or digital, when a processing block needs it | A DAC volume control, or none (digital) | In the DAC |
| Headphone correction | Parametric EQ to a target curve; imported from AutoEq or from a measurement | Processing | Available |
| Parametric EQ | At least 10 bands; peak, low and high shelf, low and high pass; frequency, gain in 0.1 dB, Q; per channel or linked | Processing | Available |
| Tone macros ("colour and texture") | Named controls on top of the EQ, in words: temperature (warm to bright), bass extension, bass texture (tight to full), note thickness (crisp to thick), presence, sibilance, air | Processing | Available (EQ underneath, visible on demand) |
| Crossfeed | Bauer crossfeed (bs2b presets and custom) for a speaker-like image | Processing | Available |
| Balance and polarity | L/R balance in 0.5 dB; channel polarity | Processing | Available |
| Loudness | ReplayGain 2.0 (track or album, −18 LUFS reference), off by default | Processing when applied | Available |
| Convolution | An impulse response (FIR) for room or headphone correction | Processing and enough compute for the filter length | Available; the longest filter is set by measurement |

Effects that synthesise content (upscaling, vinyl or tube emulation) are not in the base set. If added later, they are Personal-only and listed by name on screen.

## 3. Signal rules

- **Bit-perfect means nothing touched.** With Direct, or Monitor untouched, samples (PCM and DSD) reach the DAC unchanged (OS-020); the screen shows "Bit-perfect".
- **Processing is visible.** Any software block shows "DSP" and the list of active blocks; DSD is converted to PCM for processing, and the screen says so.
- **Headroom is automatic.** The chain is attenuated by its largest boost plus an inter-sample margin before processing, and the volume compensates, so processing never clips. The margin is set on the prototype.
- **Precision.** Processing runs in 64-bit floating point and is dithered (TPDF) to the DAC's word length.
- **Switching** is instant and glitch-free: no gap, no click (a short crossfade), within the experience budgets (T1-057).
- **Hearing safety** (SYS-012) counts the profile's effect on level: a boost raises the measured dose.

## 4. Headphones and outputs

- A profile can be tied to a headphone (by name, or detected for Bluetooth and USB devices) and an output (3.5 mm, 4.4 mm, Bluetooth, USB). Changing the output or headphone switches to its profile.
- The per-headphone profile also carries the headphone's sensitivity for the hearing-safety dose (SYS-012).

## 5. Engineering tools

Shown on Now Playing with the Monitor profile, and one step away from any profile (the "more" screen, SYS-039):

- **Level-matched A/B.** Two profiles compared at the same loudness, switched instantly, with the active one marked.
- **Mono check** and a **loudness and true-peak meter** (EBU R128 / ITU BS.1770). Meters never change the signal.
- **Signal path:** source format, active blocks, DAC filter, output, bit-perfect or not.

## 6. In the interface

- Now Playing shows the music first; the active profile's name and its Bit-perfect / DSP state are a quiet line, technical detail on demand.
- Profiles are switched from the quick settings (top edge) and from Now Playing's "more" screen; the Action long press can be set to cycle profiles (T1-061).
- Profiles import and export as plain text: the AutoEq / Equalizer APO parametric format for EQ, and a documented profile file for the whole chain. A profile that uses a block another device lacks still loads, with that block disabled.

## 7. Open points

- The CS43198's five filter responses, named from the datasheet.
- Switchable gain on the T1's output stage (useful for very sensitive IEMs and hard-to-drive headphones) is a board decision; the software block is ready for it.
- The headroom margin and the longest convolution filter, from measurement.
- Which processing engine: not chosen yet; the blocks above are the requirement, not a stack.
