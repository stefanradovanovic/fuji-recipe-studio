# Publishing Windows updates

Updates are stable GitHub Releases in the public `stefanradovanovic/fuji-recipe-studio` repository. The application version comes from `Cargo.toml`; semantic version comparison prevents an older release from being offered as an update. The first installable release is 0.2.0. The older 0.1.0 portable app must be updated manually once because it has no update button.

## Prepare a release

1. Make the changes, increase `package.version` in `Cargo.toml`, and refresh `Cargo.lock` with Cargo. Use a stable `major.minor.patch` version.
2. Run `cargo fmt --all --check` and `cargo test --release --locked --lib`. Exercise the changed app flows.
3. Update the version in the README download example. Commit and push the finished changes to `main`.
4. Write release notes in a local Markdown file. Do not put credentials, private photo paths, or photos in release assets.
5. Run the release script below with Rust, its Windows linker, GitHub CLI, and Inno Setup 7 available. A portable `ISCC.exe` works. GitHub CLI must be signed in as `stefanradovanovic` with permission to publish this repository's releases.

```powershell
.\scripts\publish-release.ps1 -NotesFile C:\path\release-notes.md -Compiler C:\path\ISCC.exe
```

For an isolated Rust/MinGW toolchain, also pass `-ToolchainRoot <folder>` to the script. It builds the app, packages the licenses and rebuildable source, creates a per-user installer, and uploads a draft release. It checks GitHub's recorded installer digest against the local file before publishing the draft as latest. If a publishing step fails, inspect the draft before retrying; the script does not overwrite existing releases or delete assets.

The required asset name is `FujiRecipeStudio-Setup-X.Y.Z-x64.exe`, with a matching `vX.Y.Z` tag. A `.sha256` companion is included for manual verification. GitHub's API must provide a `sha256:` asset digest; the updater refuses a newer release without it. Only publish a completed installer. Keep the installer's AppId stable so upgrades reuse the existing install location and uninstall entry.

## Verify the published release

```powershell
cargo run --release --locked --example check_update -- 0.1.0 --download
```

This runs the production version check and downloader against GitHub, verifies the actual asset, and discards the test download without installing it. Then run the new app and choose **Check for updates**; it should report that its installed version is current.

For an upgrade test, use a disposable Windows user profile or an isolated installation directory. Test install, upgrade, restart, and uninstall. Saved recipes live at `%LOCALAPPDATA%\FujiRecipeStudio\recipes.json`, outside the installation folder, and are deliberately left in place when uninstalling. The installer never accesses original photo files.

## Update behavior

- Checks, downloads, and progress reporting run in background workers. Checking for updates does not interrupt photo editing.
- Installation is user initiated and waits for active photo processing. It closes the editing session, performs a per-user upgrade, and relaunches the app. It does not reboot Windows or force-close other apps.
- The app uses the public GitHub API without authentication. It does not inspect credentials, invoke GitHub CLI, or need an account. GitHub's unauthenticated rate limit can temporarily require waiting between checks; manual downloads remain available on the Releases page.
- Downloads are bounded to 256 MiB, require an exact filename, use a restricted list of GitHub HTTPS hosts, and are checked for size and SHA-256 integrity before execution. Failed or unused downloads are discarded. A launched installer stays in the app's local update cache because Windows may still need its executable.
- Download integrity does not establish an independent publisher signature. The current installer has no Authenticode certificate. A future release can add Windows publisher signing before hashing and uploading the installer.
