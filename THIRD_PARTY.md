# Third-party components

Resolved versions and checksums are recorded in Cargo.lock.

- [Rawler 0.8.0](https://github.com/dnglab/dnglab/tree/v0.8.0/rawler), LGPL-2.1. Copyright Daniel Vogelbacher, Pedro Côrte-Real and contributors. Used unmodified for RAW decoding, demosaicing and camera calibration. Source: https://crates.io/api/v1/crates/rawler/0.8.0/download. The portable distribution includes this source archive and license. You may rebuild and relink the app against a modified library using Cargo. Application source is provided under MIT and does not restrict reverse engineering to debug such modifications.
- [egui / eframe](https://github.com/emilk/egui), MIT OR Apache-2.0. Native interface.
- [wgpu](https://github.com/gfx-rs/wgpu), MIT OR Apache-2.0. Graphics display.
- [image](https://github.com/image-rs/image), MIT OR Apache-2.0. Raster decoding and encoding.
- [Rayon](https://github.com/rayon-rs/rayon), MIT OR Apache-2.0. Parallel image operations.
- [rfd](https://github.com/PolyMeilex/rfd), MIT. Native file dialogs.
- cssparser, cssparser-macros, dtoa-short and selectors use MPL-2.0. Their unmodified source archives are included in the portable distribution's `source` folder.
- Other direct and transitive dependencies: see the generated inventory and license texts in the portable distribution.

Example recipe settings are from the credited Fuji X Weekly pages in README.md. Article text and photographic examples are not included.

Public RAW samples used during development came from https://raw.pixls.us/ and are not part of the app or repository.

To rebuild with a modified Rawler, extract `source/fuji-recipe-studio-source.zip` and `source/rawler-0.8.0.crate` (a gzip-compressed tar archive). Add this to the application's Cargo.toml, replacing the path with your extracted library folder:

```toml
[patch.crates-io]
rawler = { path = "../rawler-0.8.0" }
```

Run `cargo build --release --bin FujiRecipeStudio` with Rust and a Windows linker. This rebuilds and relinks the entire executable; no proprietary object files are required. The recipe parser's MPL dependencies can be replaced in the same way.
