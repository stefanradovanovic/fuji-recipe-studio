use crate::recipe::Recipe;
use anyhow::{Context, Result, bail, ensure};
use image::{ImageDecoder, ImageReader, RgbImage};
use rawler::{
    decoders::Orientation,
    imgop::{
        matrix,
        sensor::{Demosaic, SensorType, xtrans::markesteijn::XTransMarkesteijnDemosaic},
        xyz::{Illuminant, SRGB_TO_XYZ_D65},
    },
    pixarray::PixF32,
    rawimage::{RawImageData, RawPhotometricInterpretation},
};
use rayon::prelude::*;
use serde::Serialize;
use std::{
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// Cache camera-space linear pixels so WB changes never require another demosaic.
#[derive(Clone)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub full_width: usize,
    pub full_height: usize,
    pub pixels: Vec<[f32; 3]>,
    pub camera_to_rgb: [[f32; 3]; 3],
    pub as_shot_wb: [f32; 3],
    pub daylight_wb: [f32; 3],
    pub model: String,
    pub is_raw: bool,
}

pub fn decode(path: &Path) -> Result<Frame> {
    ensure!(path.is_file(), "Image file does not exist.");
    ensure!(
        path.metadata()?.len() <= 600_000_000,
        "This image is too large (600 MB limit)."
    );
    if path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("raf"))
    {
        return decode_raw(path);
    }
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    ensure!(
        u64::from(w) * u64::from(h) <= 100_000_000,
        "Image exceeds the 100 MP limit."
    );
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let rgb = image.to_rgb32f();
    let (width, height) = (rgb.width() as usize, rgb.height() as usize);
    let pixels = rgb.pixels().map(|p| p.0.map(srgb_to_linear)).collect();
    Ok(Frame {
        width,
        height,
        full_width: width,
        full_height: height,
        pixels,
        camera_to_rgb: matrix::IDENTITY_MATRIX_3,
        as_shot_wb: [1.; 3],
        daylight_wb: [1.; 3],
        model: "RGB image · assumed sRGB".into(),
        is_raw: false,
    })
}

fn decode_raw(path: &Path) -> Result<Frame> {
    let mut raw = rawler::decode_file(path).context("Could not decode this RAF file.")?;
    ensure!(
        raw.width * raw.height <= 100_000_000,
        "RAW image exceeds the 100 MP limit."
    );
    let config = match &raw.photometric {
        RawPhotometricInterpretation::Cfa(config) if config.sensor == SensorType::Xtrans => {
            config.clone()
        }
        _ => bail!(
            "This version supports Fujifilm X-Trans RAF files, including X-T5, plus JPEG, PNG and TIFF."
        ),
    };
    let (_, values) = raw
        .color_matrix_find_first([Illuminant::D65])
        .context("No daylight camera calibration found.")?;
    ensure!(values.len() == 9, "Unsupported camera color matrix.");
    let xyz_to_cam = [
        [values[0], values[1], values[2]],
        [values[3], values[4], values[5]],
        [values[6], values[7], values[8]],
    ];
    let (camera_to_rgb, daylight_wb) = daylight_calibration(xyz_to_cam)?;
    let mut as_shot_wb = [raw.wb_coeffs[0], raw.wb_coeffs[1], raw.wb_coeffs[2]];
    if as_shot_wb.iter().any(|v| !v.is_finite() || *v <= 0.) {
        as_shot_wb = daylight_wb;
    }
    let green = as_shot_wb[1];
    as_shot_wb.iter_mut().for_each(|v| *v /= green);
    let orientation = raw.orientation;
    let model = format!("{} {}", raw.clean_make, raw.clean_model);
    raw.apply_scaling()?;
    let data = match std::mem::replace(&mut raw.data, RawImageData::Integer(Vec::new())) {
        RawImageData::Float(data) => data,
        RawImageData::Integer(_) => bail!("RAW linearization did not return floating point data."),
    };
    let mosaic = PixF32::new_with(data, raw.width, raw.height);
    let roi = raw.crop_area.or(raw.active_area).unwrap_or(mosaic.rect());
    let rgb =
        XTransMarkesteijnDemosaic::new_pass_1().demosaic(&mosaic, &config.cfa, &config.colors, roi);
    let (width, height) = (rgb.width, rgb.height);
    let mut frame = Frame {
        width,
        height,
        full_width: width,
        full_height: height,
        pixels: rgb.data,
        camera_to_rgb,
        as_shot_wb,
        daylight_wb,
        model,
        is_raw: true,
    };
    orient(&mut frame, orientation);
    Ok(frame)
}

fn daylight_calibration(xyz_to_cam: [[f32; 3]; 3]) -> Result<([[f32; 3]; 3], [f32; 3])> {
    // Use the current color matrix for both conversion and white balance. Rawler's
    // legacy neutralwb() reads a deprecated, zero-filled matrix for newer cameras.
    let rgb_to_cam = matrix::multiply(&xyz_to_cam, &SRGB_TO_XYZ_D65);
    let white: [f32; 3] = rgb_to_cam.map(|row| row.iter().sum());
    ensure!(
        white.iter().all(|v| v.is_finite() && *v > 0.),
        "Invalid daylight camera calibration."
    );
    let wb = white.map(|v| white[1] / v);
    let conversion = matrix::pseudo_inverse(matrix::normalize(rgb_to_cam));
    ensure!(
        conversion.iter().flatten().all(|v| v.is_finite()),
        "Invalid camera color conversion."
    );
    Ok((conversion, wb))
}

fn orient(frame: &mut Frame, orientation: Orientation) {
    let (transpose, flip_x, flip_y) = orientation.to_flips();
    if !(transpose || flip_x || flip_y) {
        return;
    }
    let (old_w, old_h) = (frame.width, frame.height);
    let (w, h) = if transpose {
        (old_h, old_w)
    } else {
        (old_w, old_h)
    };
    let data: Vec<_> = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let (x, y) = if transpose { (y, x) } else { (x, y) };
            let sx = if flip_x { old_w - 1 - x } else { x };
            let sy = if flip_y { old_h - 1 - y } else { y };
            frame.pixels[sy * old_w + sx]
        })
        .collect();
    frame.pixels = data;
    frame.width = w;
    frame.height = h;
    frame.full_width = w;
    frame.full_height = h;
}

pub fn preview(frame: &Frame, max_side: usize) -> Frame {
    let factor = frame.width.max(frame.height).div_ceil(max_side).max(1);
    let (w, h) = (frame.width.div_ceil(factor), frame.height.div_ceil(factor));
    let pixels = (0..w * h)
        .into_par_iter()
        .map(|index| {
            let (x, y) = ((index % w) * factor, (index / w) * factor);
            let (right, bottom) = (
                (x + factor).min(frame.width),
                (y + factor).min(frame.height),
            );
            let mut total = [0.; 3];
            for row in y..bottom {
                for col in x..right {
                    let p = frame.pixels[row * frame.width + col];
                    for c in 0..3 {
                        total[c] += p[c];
                    }
                }
            }
            total.map(|v| v / ((right - x) * (bottom - y)) as f32)
        })
        .collect();
    Frame {
        pixels,
        width: w,
        height: h,
        ..frame_metadata(frame)
    }
}

fn frame_metadata(frame: &Frame) -> Frame {
    Frame {
        pixels: Vec::new(),
        width: frame.width,
        height: frame.height,
        full_width: frame.full_width,
        full_height: frame.full_height,
        camera_to_rgb: frame.camera_to_rgb,
        as_shot_wb: frame.as_shot_wb,
        daylight_wb: frame.daylight_wb,
        model: frame.model.clone(),
        is_raw: frame.is_raw,
    }
}

fn srgb_to_linear(x: f32) -> f32 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_srgb(x: f32) -> f32 {
    if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1. / 2.4) - 0.055
    }
}
fn luminance(p: [f32; 3]) -> f32 {
    p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722
}
fn strength(value: &str) -> f32 {
    match value {
        "Strong" => 1.,
        "Weak" => 0.5,
        _ => 0.,
    }
}

fn blackbody(kelvin: f32) -> [f32; 3] {
    let t = kelvin.clamp(2000., 15000.) / 100.;
    let r = if t <= 66. {
        255.
    } else {
        329.69873 * (t - 60.).powf(-0.13320476)
    };
    let g = if t <= 66. {
        99.4708 * t.ln() - 161.11957
    } else {
        288.12216 * (t - 60.).powf(-0.07551485)
    };
    let b = if t >= 66. {
        255.
    } else if t <= 19. {
        0.
    } else {
        138.51773 * (t - 10.).ln() - 305.0448
    };
    [r, g, b].map(|x| srgb_to_linear((x / 255.).clamp(0.01, 1.)))
}

/// Independent creative approximations; these are not Fujifilm LUTs or profiles.
fn profile(simulation: &str) -> (f32, f32, f32, [f32; 3], [f32; 3]) {
    match simulation {
        "Classic Chrome" => (
            0.82,
            1.12,
            0.008,
            [0.008, 0.003, -0.004],
            [-0.004, 0.003, 0.009],
        ),
        "Classic Negative" => (
            0.95,
            1.20,
            0.012,
            [0.012, -0.002, -0.008],
            [-0.014, 0.009, 0.012],
        ),
        "Nostalgic Negative" => (
            0.90,
            1.03,
            0.025,
            [0.022, 0.010, -0.015],
            [0.008, 0.005, 0.012],
        ),
        "Velvia" => (1.30, 1.20, 0., [0., 0.005, 0.008], [0.002, -0.004, 0.008]),
        "Astia" => (1.05, 0.94, 0.005, [0.005, 0., -0.003], [0., 0., 0.]),
        "Pro Neg. Std" => (0.90, 0.88, 0.012, [0.; 3], [0.; 3]),
        "Pro Neg. Hi" => (0.94, 1.14, 0.005, [0.; 3], [0.; 3]),
        "Reala Ace" => (0.98, 1.10, 0.002, [0.002, 0.003, -0.002], [0.; 3]),
        "Eterna" => (
            0.72,
            0.82,
            0.025,
            [0.006, 0.002, -0.004],
            [-0.005, 0.004, 0.008],
        ),
        "Eterna Bleach Bypass" => (0.35, 1.35, 0.005, [0.; 3], [0.001, 0.003, 0.006]),
        s if s.starts_with("Acros") => (1., 1.15, 0.003, [0.; 3], [0.; 3]),
        "Provia" => (1.04, 1.05, 0., [0.; 3], [0.; 3]),
        _ => (1., 1., 0., [0.; 3], [0.; 3]),
    }
}

pub fn render(frame: &Frame, recipe: &Recipe, exposure: f32) -> Result<RgbImage> {
    recipe.validate()?;
    ensure!(
        exposure.is_finite() && (-5.0..=5.0).contains(&exposure),
        "Exposure must be between -5 and +5 EV."
    );
    ensure!(
        frame.pixels.len() == frame.width * frame.height,
        "Invalid image buffer."
    );
    static GAMMA: OnceLock<Vec<f32>> = OnceLock::new();
    let gamma = GAMMA.get_or_init(|| {
        (0..65536)
            .map(|i| linear_to_srgb(i as f32 / 65535.))
            .collect()
    });
    let exp = 2_f32.powf(exposure);
    let wb = if recipe.is_fixed_wb() {
        frame.daylight_wb
    } else {
        frame.as_shot_wb
    };
    ensure!(
        wb.iter().all(|v| v.is_finite() && *v > 0.),
        "Invalid camera white balance."
    );
    let mut gains = [1.; 3];
    if recipe.is_fixed_wb() {
        let reference = blackbody(6500.);
        let chosen = blackbody(recipe.temperature());
        gains = std::array::from_fn(|c| (reference[c] / chosen[c]).clamp(0.2, 5.));
        let green = gains[1];
        gains.iter_mut().for_each(|g| *g /= green);
    }
    gains[0] *= 2_f32.powf(recipe.wb_red * 0.035);
    gains[2] *= 2_f32.powf(recipe.wb_blue * 0.035);
    let (saturation, contrast, fade, warm, cool) = profile(&recipe.simulation);
    let tone: Vec<f32> = (0..4096)
        .map(|i| {
            let t = i as f32 / 4095.;
            let mut y = t + (contrast - 1.) * 3. * (t - 0.5) * t * (1. - t);
            y += recipe.highlights * 0.16 * t * t * (1. - t);
            y -= recipe.shadows * 0.18 * t * (1. - t) * (1. - t);
            y += fade * (1. - t).powi(3);
            let dr = match recipe.dynamic_range.as_str() {
                "DR400" => 0.13,
                "DR200" => 0.065,
                _ => 0.,
            };
            y -= dr * t * t * (1. - t);
            y.clamp(0., 1.)
        })
        .collect();
    let mono = recipe.simulation.starts_with("Acros")
        || recipe.simulation.starts_with("Monochrome")
        || recipe.simulation == "Sepia";
    let mono_weights = if recipe.simulation.ends_with("+R") {
        [0.60, 0.32, 0.08]
    } else if recipe.simulation.ends_with("+G") {
        [0.15, 0.78, 0.07]
    } else if recipe.simulation.ends_with("+Y") {
        [0.38, 0.53, 0.09]
    } else {
        [0.2126, 0.7152, 0.0722]
    };
    let chrome = strength(&recipe.color_chrome) * 0.12;
    let blue = strength(&recipe.blue_chrome) * 0.17;
    let mut pixels: Vec<[f32; 3]> = frame
        .pixels
        .par_iter()
        .map(|p| {
            let balanced = std::array::from_fn(|c| p[c] * wb[c]);
            let linear = matrix::multiply_row1(&frame.camera_to_rgb, &balanced);
            let mut p: [f32; 3] = std::array::from_fn(|c| {
                gamma[(linear[c] * exp * gains[c] * 65535.).clamp(0., 65535.) as usize]
            });
            if mono {
                let gray: f32 = (0..3).map(|c| p[c] * mono_weights[c]).sum();
                p = [gray; 3];
            } else {
                let y = luminance(p);
                let saturation = saturation * (1. + recipe.color * 0.065);
                for c in 0..3 {
                    p[c] =
                        y + (p[c] - y) * saturation + warm[c] * y * y + cool[c] * (1. - y).powi(2);
                }
                if recipe.simulation == "Classic Negative" {
                    let green = (p[1] - (p[0] + p[2]) * 0.5).max(0.);
                    p[0] -= green * 0.10;
                    p[2] += green * 0.12;
                }
                let max = p[0].max(p[1]).max(p[2]);
                let min = p[0].min(p[1]).min(p[2]);
                let density =
                    1. - chrome * (max - min).max(0.) - blue * (p[2] - p[0].max(p[1])).max(0.);
                p.iter_mut().for_each(|v| *v *= density);
            }
            let y = luminance(p).max(0.00001);
            let mapped = tone[(y * 4095.).clamp(0., 4095.) as usize];
            for c in 0..3 {
                p[c] = (p[c] * mapped / y).clamp(0., 1.);
            }
            if mono {
                let warmth = recipe.mono_wc * 0.0015
                    + if recipe.simulation == "Sepia" {
                        0.065
                    } else {
                        0.
                    };
                let tint = recipe.mono_mg * 0.0015;
                p[0] += warmth;
                p[2] -= warmth;
                p[1] -= tint;
            }
            p
        })
        .collect();
    let scale = frame.width as f32 / frame.full_width as f32;
    if recipe.clarity != 0. || recipe.sharpness != 0. || recipe.noise_reduction > -4. {
        let luma: Vec<f32> = pixels.par_iter().map(|p| luminance(*p)).collect();
        let fine = box_blur(
            &luma,
            frame.width,
            frame.height,
            (scale * 1.5).round().max(1.) as usize,
        );
        let coarse = if recipe.clarity != 0. {
            box_blur(
                &luma,
                frame.width,
                frame.height,
                (scale * 24.).round().max(2.) as usize,
            )
        } else {
            Vec::new()
        };
        pixels.par_iter_mut().enumerate().for_each(|(i, p)| {
            let detail = luma[i] - fine[i];
            let noise =
                ((recipe.noise_reduction + 4.) / 8.) * 0.35 * (1. - detail.abs() / 0.08).max(0.);
            let mut delta = detail * (recipe.sharpness * 0.18 - noise);
            if !coarse.is_empty() {
                delta += (luma[i] - coarse[i]) * recipe.clarity * 0.13;
            }
            p.iter_mut().for_each(|v| *v += delta);
        });
    }
    let amount = strength(&recipe.grain) * 0.026 * scale.sqrt();
    let grain_size = (if recipe.grain_size == "Large" {
        3. * scale
    } else {
        scale
    })
    .round()
    .max(1.) as usize;
    let mut result = vec![0_u8; frame.width * frame.height * 3];
    result
        .par_chunks_exact_mut(3)
        .enumerate()
        .for_each(|(i, out)| {
            let p = pixels[i];
            let x = (i % frame.width) / grain_size;
            let y = (i / frame.width) / grain_size;
            let grain =
                noise(x as u32, y as u32) * amount * (0.35 + luminance(p).clamp(0., 1.) * 0.65);
            for c in 0..3 {
                out[c] = ((p[c] + grain).clamp(0., 1.) * 255. + 0.5) as u8;
            }
        });
    RgbImage::from_raw(frame.width as u32, frame.height as u32, result)
        .context("Invalid rendered dimensions.")
}

fn noise(x: u32, y: u32) -> f32 {
    let mut v = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(1397);
    v = (v ^ (v >> 13)).wrapping_mul(1274126177);
    v ^= v >> 16;
    (v & 65535) as f32 / 32767.5 - 1.
}

/// Sliding sums make blur O(pixels), independent of the clarity radius.
fn box_blur(input: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let mut horizontal = vec![0.; input.len()];
    horizontal
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, out)| {
            let row = &input[y * w..(y + 1) * w];
            let mut sum: f32 = row[..(radius + 1).min(w)].iter().sum();
            for (x, value) in out.iter_mut().enumerate() {
                *value = sum / ((x + radius + 1).min(w) - x.saturating_sub(radius)) as f32;
                if x >= radius {
                    sum -= row[x - radius];
                }
                if x + radius + 1 < w {
                    sum += row[x + radius + 1];
                }
            }
        });
    let mut out = vec![0.; input.len()];
    let mut sums = vec![0.; w];
    for row in horizontal.chunks(w).take((radius + 1).min(h)) {
        for x in 0..w {
            sums[x] += row[x];
        }
    }
    for y in 0..h {
        let count = ((y + radius + 1).min(h) - y.saturating_sub(radius)) as f32;
        for x in 0..w {
            out[y * w + x] = sums[x] / count;
        }
        if y >= radius {
            for x in 0..w {
                sums[x] -= horizontal[(y - radius) * w + x];
            }
        }
        if y + radius + 1 < h {
            for x in 0..w {
                sums[x] += horizontal[(y + radius + 1) * w + x];
            }
        }
    }
    out
}

#[derive(Serialize)]
struct ExportSettings<'a> {
    application: &'static str,
    version: &'static str,
    source_file: String,
    exposure_ev: f32,
    recipe: &'a Recipe,
    rendering: &'static str,
}

pub fn export(
    frame: &Frame,
    source: &Path,
    destination: &Path,
    recipe: &Recipe,
    exposure: f32,
) -> Result<PathBuf> {
    let extension = destination
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    ensure!(
        matches!(extension.as_str(), "jpg" | "jpeg" | "png"),
        "Choose a .jpg or .png filename."
    );
    ensure!(
        !destination.exists(),
        "That file already exists. Choose a new filename; originals are never overwritten."
    );
    let sidecar = PathBuf::from(format!("{}.recipe.json", destination.display()));
    ensure!(
        !sidecar.exists(),
        "A recipe sidecar already exists at that destination. Choose a new filename."
    );
    let image = render(frame, recipe, exposure)?;
    let parent = destination.parent().unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        if extension == "png" {
            image.write_to(&mut writer, image::ImageFormat::Png)?;
        } else {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 95)
                .encode_image(&image)?;
        }
        writer.flush()?;
    }
    temporary.as_file().sync_all()?;
    let mut settings = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(
        settings.as_file_mut(),
        &ExportSettings {
            application: "Fuji Recipe Studio",
            version: env!("CARGO_PKG_VERSION"),
            source_file: source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            exposure_ev: exposure,
            recipe,
            rendering: "Independent approximate film rendering; not Fujifilm's processing engine.",
        },
    )?;
    settings.flush()?;
    settings.as_file().sync_all()?;
    settings.persist_noclobber(&sidecar)?;
    if let Err(error) = temporary.persist_noclobber(destination) {
        let _ = std::fs::remove_file(&sidecar);
        return Err(error.into());
    }
    Ok(destination.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> Frame {
        Frame {
            width: 3,
            height: 2,
            full_width: 3,
            full_height: 2,
            pixels: vec![[0.1, 0.2, 0.3]; 6],
            camera_to_rgb: matrix::IDENTITY_MATRIX_3,
            as_shot_wb: [1.; 3],
            daylight_wb: [1.; 3],
            model: "Test".into(),
            is_raw: false,
        }
    }
    #[test]
    fn neutral_preserves_srgb_and_render_does_not_modify_source() {
        let frame = frame();
        let before = frame.pixels.clone();
        let out = render(&frame, &Recipe::default(), 0.).unwrap();
        let expected = [0.1, 0.2, 0.3].map(|x| (linear_to_srgb(x) * 255.).round() as u8);
        for (a, b) in out.get_pixel(0, 0).0.into_iter().zip(expected) {
            assert!(a.abs_diff(b) <= 1);
        }
        assert_eq!(frame.pixels, before);
    }
    #[test]
    fn monochrome_channels_equal_and_exposure_has_effect() {
        let frame = frame();
        let r = Recipe {
            simulation: "Acros+G".into(),
            ..Default::default()
        };
        let a = render(&frame, &r, 0.).unwrap();
        let b = render(&frame, &r, 1.).unwrap();
        assert_eq!(a.get_pixel(0, 0)[0], a.get_pixel(0, 0)[1]);
        assert!(b.get_pixel(0, 0)[0] > a.get_pixel(0, 0)[0]);
    }
    #[test]
    fn daylight_calibration_preserves_a_camera_neutral_and_rejects_missing_data() {
        let xyz_to_cam = [[1.2, -0.2, -0.1], [-0.3, 1.1, 0.1], [0.01, 0.2, 0.6]];
        let (conversion, wb) = daylight_calibration(xyz_to_cam).unwrap();
        let camera_white =
            matrix::multiply(&xyz_to_cam, &SRGB_TO_XYZ_D65).map(|row| row.iter().sum::<f32>());
        let balanced = std::array::from_fn(|c| camera_white[c] * wb[c]);
        let rgb = matrix::multiply_row1(&conversion, &balanced);
        assert!((rgb[0] - rgb[1]).abs() < 1e-5 && (rgb[1] - rgb[2]).abs() < 1e-5);
        assert!(daylight_calibration([[0.; 3]; 3]).is_err());
        let mut f = frame();
        f.daylight_wb = [f32::NAN; 3];
        assert!(
            render(
                &f,
                &Recipe {
                    white_balance: "Daylight".into(),
                    ..Default::default()
                },
                0.
            )
            .is_err()
        );
    }
    #[test]
    fn portrait_orientation_and_preview_keep_dimensions() {
        let mut f = frame();
        f.pixels = (0..6).map(|i| [i as f32; 3]).collect();
        orient(&mut f, Orientation::Rotate90);
        assert_eq!((f.width, f.height), (2, 3));
        assert_eq!(
            f.pixels.iter().map(|p| p[0] as i32).collect::<Vec<_>>(),
            [3, 0, 4, 1, 5, 2]
        );
        let p = preview(&f, 2);
        assert_eq!((p.width, p.height), (1, 2));
    }
    #[test]
    fn export_never_clobbers_existing_output_or_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("original.jpg");
        std::fs::write(&source, b"unchanged").unwrap();
        assert!(export(&frame(), &source, &source, &Recipe::default(), 0.).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), b"unchanged");
        let output = dir.path().join("result.jpg");
        export(&frame(), &source, &output, &Recipe::default(), 0.).unwrap();
        assert!(image::open(&output).is_ok());
        assert!(export(&frame(), &source, &output, &Recipe::default(), 0.).is_err());
    }
    #[test]
    fn blur_preserves_constant_images_at_edges() {
        assert!(
            box_blur(&vec![0.5; 30], 6, 5, 3)
                .iter()
                .all(|v| (*v - 0.5).abs() < 1e-6)
        );
    }
}
