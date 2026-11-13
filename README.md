# Fuji Recipe Studio

A standalone Windows app for applying **approximate Fujifilm recipe looks** to X-T5 RAW photographs. Written in Rust, with an egui/wgpu interface and local image processing. Photo editing needs no camera connection, Python installation, account, or photo upload.

## Install and update

Download `FujiRecipeStudio-Setup-0.2.0-x64.exe` from [Releases](https://github.com/stefanradovanovic/fuji-recipe-studio/releases/latest), run it, and launch **Fuji Recipe Studio** from the Start menu. The installer supports Windows 10/11 x64, installs for the current user without administrator rights, and adds an entry to Windows' installed apps for uninstalling. A desktop shortcut is optional.

In the app, choose **Check for updates** in the top bar. If a newer stable release is available, choose **Download update**, then **Install and restart**. Save recipe edits and export any photo you want to keep before restarting: the current editing session is not restored. Your saved recipe library and original photographs are preserved by updates and uninstalling.

Updates are available from public GitHub Releases. No GitHub account, token, or GitHub CLI installation is needed to check or download updates. Manual installation from the Releases page also works. Photo editing works offline.

Update checks happen only when requested. Downloads stay on GitHub HTTPS hosts; the app checks the exact asset name, size, and GitHub's SHA-256 digest, then rechecks the file before launching Setup. These are integrity checks, not Authenticode signing: this release's Windows installer is unsigned.

## Use the app

1. Launch **Fuji Recipe Studio** (or open the portable `FujiRecipeStudio.exe`) on Windows 10/11 x64 with a DirectX 12 compatible graphics driver.
2. Choose **Open photos**, or drag in a `.RAF`. Select opened photographs in the photo strip.
3. Pick a recipe. **Compare** gives you a draggable divider between the neutral RAW and the recipe.
4. Adjust exposure and settings. **Save as recipe** adds a personal preset.
5. **Export image** saves a full-resolution JPEG or PNG plus a `.recipe.json` settings sidecar. Existing files are never overwritten.

Shortcuts: `Ctrl+O` opens photos, `Ctrl+I` imports a recipe, `Ctrl+E` exports.

**Import recipe** accepts a specific recipe URL or pasted settings. A recipe index page is not an individual recipe. Parsed settings are shown for review before saving. Pages requiring login, JavaScript, or paid access may need manual text entry. Imported content is treated only as data.

Custom recipes are stored in `%LOCALAPPDATA%\FujiRecipeStudio\recipes.json`. An unreadable library is preserved rather than overwritten.

## Included recipes

Published settings by Ritchie Roesch / Fuji X Weekly. Our rendering is an independent approximation. Source links remain in the app; article bodies and sample photographs are not bundled.

- [Kodak Gold 200](https://fujixweekly.com/2023/10/24/kodak-gold-200-fujifilm-x-t5-x-trans-v-film-simulation-recipe/)
- [Kodak Portra 400 v2](https://fujixweekly.com/2022/12/16/kodak-portra-400-v2-fujifilm-x-t5-x-trans-v-film-simulation-recipe/)
- [Pacific Blues](https://fujixweekly.com/2022/12/06/pacific-blues-fujifilm-x-t5-x-trans-v-film-simulation-recipe/)
- [Classic B&W](https://fujixweekly.com/2025/08/03/classic-bw-film-simulation-recipe/)

## Performance

RAW decoding uses `rawler`, camera calibration, black-level scaling and one-pass Markesteijn demosaicing. Linear camera-space pixels stay cached: choosing recipes, changing white balance and exporting do not repeat decoding. A separate linear preview is limited to 1,600 pixels on its longest side.

Rayon parallelizes image processing. Gamma/tone lookup tables and sliding-sum blur reduce repeated work. Decode, render, import and export run outside the UI thread. Rapid edits are coalesced, and stale results cannot replace current settings. Only the active photo is cached at full resolution.

wgpu accelerates the interface and display. **Image processing runs on the CPU**, not GPU compute shaders. The preview time shown in the UI measures processing, excluding initial RAW decoding and screen presentation.

Measured on an Intel Core Ultra 7 255H with 12 processing threads and a public X-T5 lossless RAF: 2.42 s to decode 7,728 × 5,152 pixels, 48–54 ms for the color recipes at a 1,546 × 1,031 preview size, and 3.31 s for a full-resolution Gold 200 JPEG. These are local sample measurements, not guarantees; photos, hardware and concurrent work affect timing.

## Limits of this version

- Film simulations are hand-tuned creative approximations, not measured matches to Fujifilm's proprietary processing.
- Auto WB uses the recorded camera white balance; fixed Kelvin and presets are approximate. DR200/DR400 shape the tone curve and cannot recover clipped highlights. Auto DR uses the normal curve. ISO and exposure recommendations are shooting advice, not automatically applied adjustments.
- Noise reduction is mild luminance smoothing. Grain and fine detail differ between reduced previews and full-size exports.
- Input: Fujifilm X-Trans RAF (including X-T5), JPEG, PNG and TIFF. Raster inputs are assumed sRGB; embedded wide-gamut profiles are not converted. RAW is preferred.
- Output: 8-bit sRGB-encoded JPEG (quality 95) or PNG. Original EXIF/GPS metadata is not copied.
- Lens correction, crop tools and batch export are not included yet. Original files are read only. There are no camera-control functions.

## Build and verify

Install Rust 1.89+ and a Windows linker (MSVC Build Tools, or MinGW-w64 for the GNU target).

```powershell
cargo test --release --locked --lib
cargo build --release --locked --bin FujiRecipeStudio
.\target\release\FujiRecipeStudio.exe
```

`scripts/build.ps1` creates a portable folder under `dist`. For an isolated Rust/MinGW toolchain with `cargo`, `rustup`, and `mingw64` subfolders, pass `-ToolchainRoot <folder>`.

To build the Windows installer, first build/package the app, then run `scripts/installer.ps1 -Compiler <path-to-ISCC.exe>` with Inno Setup 7. The result and its SHA-256 file are written to `dist/installer`. See [Publishing updates](docs/releases.md) for the repeatable release process.

Real-file verification and benchmarks:

```powershell
cargo run --release --locked --example verify -- C:\Photos\sample.RAF .local\verification
```

Add `--fetch` to verify the four live recipe pages. Use a fresh output directory per run: verification follows the same no-overwrite policy. RAW samples and generated exports are not committed.

Application source is MIT licensed. `rawler` is LGPL-2.1; see `THIRD_PARTY.md`. Source and Cargo.lock are provided for rebuilding and relinking. This app is not affiliated with Fujifilm or Fuji X Weekly.
