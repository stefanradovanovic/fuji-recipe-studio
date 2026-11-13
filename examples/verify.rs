//! End-to-end verification and timing against a real input file.
//! cargo run --release --example verify -- INPUT.RAF OUTPUT_DIRECTORY [--fetch]
use anyhow::{Context, Result, ensure};
use fuji_recipe_studio::{engine, import, recipe};
use serde_json::json;
use std::{path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let threads = std::thread::available_parallelism()?
        .get()
        .saturating_sub(1)
        .clamp(1, 12);
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let input = PathBuf::from(args.first().context("Pass an input image.")?);
    let output = PathBuf::from(args.get(1).context("Pass an output directory.")?);
    std::fs::create_dir_all(&output)?;
    let start = Instant::now();
    let full = engine::decode(&input)?;
    ensure!(
        full.daylight_wb
            .iter()
            .chain(full.as_shot_wb.iter())
            .all(|v| v.is_finite() && *v > 0.),
        "Invalid RAW calibration"
    );
    let decode_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let small = engine::preview(&full, 1600);
    let downsample_ms = start.elapsed().as_secs_f64() * 1000.;
    println!(
        "Decoded {} {}x{} in {:.0} ms; preview {}x{} in {:.0} ms",
        full.model, full.width, full.height, decode_ms, small.width, small.height, downsample_ms
    );
    println!(
        "As-shot WB {:?}, daylight WB {:?}",
        full.as_shot_wb, full.daylight_wb
    );
    let recipes = recipe::builtins();
    let mut timings = Vec::new();
    for (index, recipe) in recipes.iter().enumerate() {
        // Warm the shared gamma LUT before timing interactive recipe changes.
        let _ = engine::render(&small, recipe, 0.)?;
        let mut runs = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            let _ = engine::render(&small, recipe, 0.)?;
            runs.push(start.elapsed().as_secs_f64() * 1000.);
        }
        runs.sort_by(f64::total_cmp);
        let rendered = engine::render(&small, recipe, 0.)?;
        let (min, max) = rendered
            .pixels()
            .map(|p| (u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2])) / 3)
            .fold((255, 0), |(low, high), v| (low.min(v), high.max(v)));
        ensure!(
            max - min > 20,
            "Render has no useful tonal range: {}",
            recipe.name
        );
        rendered.save(output.join(format!("preview-{index}.png")))?;
        println!("{} median {:.1} ms", recipe.name, runs[2]);
        timings.push(json!({"recipe":recipe.name,"median_preview_ms":runs[2],"runs_ms":runs}));
    }
    let destination = output.join("full-resolution.jpg");
    let start = Instant::now();
    engine::export(&full, &input, &destination, &recipes[1], 0.)?;
    let export_ms = start.elapsed().as_secs_f64() * 1000.;
    let dimensions = image::image_dimensions(&destination)?;
    ensure!(
        dimensions == (full.width as u32, full.height as u32),
        "Export resolution mismatch."
    );
    println!(
        "Full export {}x{} {:.0} ms",
        dimensions.0, dimensions.1, export_ms
    );
    if args.iter().any(|a| a == "--fetch") {
        for expected in recipes.iter().skip(1) {
            let imported = import::fetch_recipe(&expected.source)?;
            ensure!(
                imported.recipe.simulation == expected.simulation,
                "Simulation import mismatch for {}",
                expected.name
            );
            ensure!(
                imported.recipe.wb_red == expected.wb_red
                    && imported.recipe.wb_blue == expected.wb_blue,
                "WB import mismatch"
            );
            ensure!(
                imported.recipe.highlights == expected.highlights
                    && imported.recipe.shadows == expected.shadows,
                "Tone import mismatch"
            );
            ensure!(
                imported.recipe.grain == expected.grain
                    && imported.recipe.grain_size == expected.grain_size,
                "Grain import mismatch"
            );
            ensure!(
                imported.recipe.white_balance == expected.white_balance,
                "WB mode import mismatch"
            );
            ensure!(
                imported.recipe.color_chrome == expected.color_chrome
                    && imported.recipe.blue_chrome == expected.blue_chrome,
                "Chrome import mismatch"
            );
            println!("Live recipe import verified: {}", imported.recipe.name);
        }
    }
    std::fs::write(
        output.join("benchmark.json"),
        serde_json::to_vec_pretty(&json!({
            "threads":threads, "camera": full.model, "full_resolution":[full.width,full.height],
            "preview_resolution":[small.width,small.height],"decode_ms":decode_ms,"downsample_ms":downsample_ms,
            "full_jpeg_export_ms":export_ms,"recipes":timings
        }))?,
    )?;
    Ok(())
}
