# Verification

## Version 0.2: installer and updates

Verified on Windows x64 on 14 November 2026.

- All 16 release-profile library tests pass. The five updater tests cover semantic version ordering/no downgrade, invalid release metadata, download truncation/tampering, allowed HTTPS redirects, and checking the downloaded file again before execution.
- The PowerShell packaging/publishing scripts parse successfully. The installer is compiled with the portable Inno Setup 7.1 compiler; its compiler signature was verified before use.
- Before making the repository public, inspected its tracked file list and scanned all 40 historical blobs across three commits for credential patterns, private keys, local user directories, and photograph/sensitive-file extensions. No matches were found. An unauthenticated GitHub API request confirmed public access.
- Installed 0.1.0 into an isolated folder, then upgraded it to 0.2.0 with the same silent/restart flags used by the updater. The installer retained the chosen directory and relaunched the app. The installed executable's SHA-256 matched the release build.
- Observed the native **Check for updates** dialog with installed version 0.2.0 and its recoverable missing-release message before publishing.
- Uninstalled the isolated installation successfully. The uninstall entry and installed executable were removed; the test recipe library, actual user profile, and original RAF remained unchanged.

## Version 0.1: image processing

Verified on Windows x64 with an Intel Core Ultra 7 255H, 32 GB RAM, Rust 1.100.0 GNU and a MinGW-w64 linker. Tests were run on 13–14 November 2026.

### Automated checks

- All 11 library tests passed in the release profile. Coverage includes recipe validation/import, library persistence, daylight camera calibration, neutral and monochrome rendering, image orientation, preview dimensions, blur edges and refusing to overwrite source/output files.
- `cargo fmt --all --check` and `git diff --check` passed.
- Release binary built with an explicit Windows DirectX 12 wgpu backend.
- Live URL imports from the four credited Fuji X Weekly pages matched the bundled simulation, WB mode and shifts, highlight/shadow, grain and Color Chrome settings.
- Real X-T5 lossless and compressed RAF files decoded and produced 7,728 × 5,152 JPEG exports plus recipe sidecars.
- The lossless RAW's SHA-256 remained `B2A067E7FDA16338F15C2532CAA12B238B2415DC1C75A96A6D24D3445CFD0AE3` after processing and UI export.

Sample inputs: public raw.pixls.us X-T5 DSCF0021.RAF (lossless) and DSCF0022.RAF (compressed). Samples and derived photographs are excluded from the repository and portable package.

### Native interface checks

- Opened a real RAF in the Windows app and inspected the neutral/recipe comparison.
- Selected Classic B&W and observed the monochrome preview.
- Exported through the native Save dialog; verified that the JPEG and settings sidecar were created.
- Fetched Pacific Blues by URL, reviewed the parsed settings, saved it to an isolated test library, and observed it applied in the interface.
- Verified that the saved library contained the expected recipe settings.
- Confirmed the final build's Ctrl+I shortcut opens recipe import, and captured the rendered interface successfully with exit code 0.

### Sample timing

12 Rayon threads; 1,546 × 1,031 preview; median of five render calls after warming the lookup table. Disk cache was not deliberately cleared. UI presentation is excluded.

| Operation | Time |
| --- | ---: |
| Lossless RAF decode and demosaic | 2,424 ms |
| Preview downsampling | 21 ms |
| Neutral RAW preview | 24 ms |
| Kodak Gold 200 preview | 54 ms |
| Kodak Portra 400 v2 preview | 48 ms |
| Pacific Blues preview | 51 ms |
| Classic B&W preview | 46 ms |
| Full-resolution Gold 200 JPEG export | 3,311 ms |

These are measurements of these files on this PC. They do not establish an accuracy match to Fujifilm's rendering. The profiles are independent creative approximations, and visual calibration against matched camera JPEGs remains future work.
