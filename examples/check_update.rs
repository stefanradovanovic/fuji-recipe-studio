//! Exercises the production release check and verified download without running it.
//! cargo run --release --example check_update -- 0.1.0 --download
use anyhow::Result;
use fuji_recipe_studio::updates::{self, Check};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let current = args
        .first()
        .map(String::as_str)
        .unwrap_or(env!("CARGO_PKG_VERSION"));
    match updates::check(current)? {
        Check::Current { latest } => println!("Up to date. Latest stable: {latest}"),
        Check::Available(release) => {
            println!(
                "Update available: {} ({:.1} MB)",
                release.version,
                release.size_mb()
            );
            if args.iter().any(|arg| arg == "--download") {
                let downloaded = updates::download(&release, |_, _| {})?;
                println!(
                    "Downloaded and verified {} bytes. Temporary test download will be removed.",
                    downloaded.path().metadata()?.len()
                );
            }
        }
    }
    Ok(())
}
