# Additional dependency notices

Some crates omit their monorepo's license file from their published archive. These files supplement the license texts collected from the Cargo cache by `scripts/package.ps1`.

- `egui-0.32.3-MIT.txt`: ecolor, emath, epaint, egui, egui-wgpu, egui-winit and eframe. Retrieved from emilk/egui commit af96e0373c18477b77236e2bfc89735af007b1c2.
- `wgpu-naga-25-MIT.txt`: shared wgpu license, including naga and wgpu-core-deps-windows-linux-android. From wgpu 25.0.2's archive.
- `profiling-1.0.18-MIT.txt`: from aclysma/profiling commit 8271551172eb6fa4cba47369aedd93790c623df9.
- `clipboard-win-5.4.3-BOOST.txt`: from DoumanAsh/clipboard-win commit ed27008ae5d1f10cfbb266de039f0fb35fd05a56.
- `alloc-stdlib-0.2.4-BSD.txt`: shared rust-alloc-no-stdlib license, from alloc-no-stdlib 2.0.4's archive.
- `match_token-0.35.0-MIT.txt`: shared html5ever project license, from html5ever 0.35.0's archive.
- `selectors-0.31.0-MPL.txt`: Mozilla Public License 2.0 text from cssparser 0.35.0's archive. The unmodified selectors source is also distributed.
- `hexf-parse-0.2.1-CC0.txt`: CC0 1.0 terms from https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt. Author: Kang Seonghoon.
- `Apache-2.0.txt`: Apache License 2.0, selected for `fxhash` 0.2.1 (author cbreeden) and `mac` 0.1.1 (author Jonathan Reem), which declare MIT/Apache-2.0 in their published manifests. Also covers components of Rust's standard library under that license.

The fxhash source carries this notice: Copyright 2015 The Rust Project Developers.

The complete resolved package inventory is generated as `licenses/DEPENDENCIES.md` in the portable distribution. Embedded font license notices are copied from epaint_default_fonts, including Ubuntu, Noto Emoji, Hack and the emoji icon font.
