use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

pub const SIMULATIONS: &[&str] = &[
    "Neutral",
    "Provia",
    "Velvia",
    "Astia",
    "Classic Chrome",
    "Classic Negative",
    "Nostalgic Negative",
    "Reala Ace",
    "Pro Neg. Std",
    "Pro Neg. Hi",
    "Eterna",
    "Eterna Bleach Bypass",
    "Acros",
    "Acros+R",
    "Acros+Y",
    "Acros+G",
    "Monochrome",
    "Monochrome+R",
    "Monochrome+Y",
    "Monochrome+G",
    "Sepia",
];
pub const WHITE_BALANCES: &[&str] = &[
    "As shot",
    "Auto",
    "Daylight",
    "Shade",
    "Cloudy",
    "Kelvin",
    "Incandescent",
    "Fluorescent 1",
    "Fluorescent 2",
    "Fluorescent 3",
];
pub const STRENGTHS: &[&str] = &["Off", "Weak", "Strong"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Recipe {
    pub name: String,
    pub simulation: String,
    pub white_balance: String,
    pub kelvin: f32,
    pub wb_red: f32,
    pub wb_blue: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub color: f32,
    pub sharpness: f32,
    pub clarity: f32,
    pub noise_reduction: f32,
    pub grain: String,
    pub grain_size: String,
    pub color_chrome: String,
    pub blue_chrome: String,
    pub dynamic_range: String,
    pub mono_wc: f32,
    pub mono_mg: f32,
    pub exposure_hint: String,
    pub iso_hint: String,
    pub author: String,
    pub source: String,
    pub notes: String,
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            name: "My recipe".into(),
            simulation: "Neutral".into(),
            white_balance: "As shot".into(),
            kelvin: 5500.,
            wb_red: 0.,
            wb_blue: 0.,
            highlights: 0.,
            shadows: 0.,
            color: 0.,
            sharpness: 0.,
            clarity: 0.,
            noise_reduction: -4.,
            grain: "Off".into(),
            grain_size: "Small".into(),
            color_chrome: "Off".into(),
            blue_chrome: "Off".into(),
            dynamic_range: "DR100".into(),
            mono_wc: 0.,
            mono_mg: 0.,
            exposure_hint: String::new(),
            iso_hint: String::new(),
            author: String::new(),
            source: String::new(),
            notes: String::new(),
        }
    }
}

impl Recipe {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 200,
            "Recipe name must contain 1–200 characters."
        );
        for (label, value, choices) in [
            ("film simulation", self.simulation.as_str(), SIMULATIONS),
            ("white balance", self.white_balance.as_str(), WHITE_BALANCES),
            ("grain", self.grain.as_str(), STRENGTHS),
            (
                "grain size",
                self.grain_size.as_str(),
                &["Small", "Large"][..],
            ),
            ("Color Chrome", self.color_chrome.as_str(), STRENGTHS),
            ("Color Chrome Blue", self.blue_chrome.as_str(), STRENGTHS),
            (
                "dynamic range",
                self.dynamic_range.as_str(),
                &["Auto", "DR100", "DR200", "DR400"][..],
            ),
        ] {
            ensure!(choices.contains(&value), "Unsupported {label}: {value}");
        }
        for (label, value, low, high) in [
            ("Kelvin", self.kelvin, 2000., 15000.),
            ("Red shift", self.wb_red, -9., 9.),
            ("Blue shift", self.wb_blue, -9., 9.),
            ("Highlights", self.highlights, -2., 4.),
            ("Shadows", self.shadows, -2., 4.),
            ("Color", self.color, -4., 4.),
            ("Sharpness", self.sharpness, -4., 4.),
            ("Clarity", self.clarity, -5., 5.),
            ("Noise reduction", self.noise_reduction, -4., 4.),
            ("Monochrome warmth", self.mono_wc, -18., 18.),
            ("Monochrome tint", self.mono_mg, -18., 18.),
        ] {
            ensure!(
                value.is_finite() && (low..=high).contains(&value),
                "{label} must be between {low} and {high}."
            );
        }
        for value in [
            &self.author,
            &self.source,
            &self.notes,
            &self.exposure_hint,
            &self.iso_hint,
        ] {
            ensure!(value.len() <= 10_000, "Recipe text is too long.");
        }
        Ok(())
    }

    pub fn is_fixed_wb(&self) -> bool {
        self.white_balance != "As shot" && self.white_balance != "Auto"
    }

    pub fn temperature(&self) -> f32 {
        match self.white_balance.as_str() {
            "Daylight" => 5500.,
            "Shade" => 7500.,
            "Cloudy" => 6500.,
            "Incandescent" => 2850.,
            "Fluorescent 1" => 6400.,
            "Fluorescent 2" => 5000.,
            "Fluorescent 3" => 4100.,
            "Kelvin" => self.kelvin,
            _ => 5500.,
        }
    }
}

pub fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+')
        .collect()
}

pub fn simulation(value: &str) -> Result<String> {
    let key = normalize(value);
    if let Some(sim) = SIMULATIONS.iter().find(|s| normalize(s) == key) {
        return Ok((*sim).into());
    }
    let alias = match key.as_str() {
        "classicneg" => "Classic Negative",
        "nostalgicneg" => "Nostalgic Negative",
        "proviastandard" | "standard" => "Provia",
        "velviavivid" => "Velvia",
        "astiasoft" => "Astia",
        "pronegstandard" => "Pro Neg. Std",
        "proneghigh" => "Pro Neg. Hi",
        "eternacinema" => "Eterna",
        _ => bail!("Unsupported film simulation: {value}"),
    };
    Ok(alias.into())
}

pub fn builtins() -> Vec<Recipe> {
    let common = Recipe {
        grain: "Strong".into(),
        grain_size: "Small".into(),
        sharpness: -2.,
        clarity: -2.,
        dynamic_range: "DR400".into(),
        author: "Ritchie Roesch / Fuji X Weekly".into(),
        iso_hint: "Auto, up to ISO 6400".into(),
        ..Default::default()
    };
    vec![
        Recipe { name: "Neutral RAW".into(), ..Default::default() },
        Recipe { name: "Kodak Gold 200".into(), simulation: "Classic Chrome".into(), white_balance: "Daylight".into(),
            wb_red: 4., wb_blue: -5., highlights: -1.5, shadows: 0.5, color: 3., color_chrome: "Weak".into(),
            exposure_hint: "+2/3 to +1 EV (shooting advice)".into(),
            source: "https://fujixweekly.com/2023/10/24/kodak-gold-200-fujifilm-x-t5-x-trans-v-film-simulation-recipe/".into(),
            ..common.clone() },
        Recipe { name: "Kodak Portra 400 v2".into(), simulation: "Classic Chrome".into(), white_balance: "Kelvin".into(),
            kelvin: 5200., wb_red: 1., wb_blue: -6., shadows: -2., color: 2., color_chrome: "Strong".into(),
            exposure_hint: "+1/3 to +1 EV (shooting advice)".into(),
            source: "https://fujixweekly.com/2022/12/16/kodak-portra-400-v2-fujifilm-x-t5-x-trans-v-film-simulation-recipe/".into(),
            ..common.clone() },
        Recipe { name: "Pacific Blues".into(), simulation: "Classic Negative".into(), white_balance: "Kelvin".into(),
            kelvin: 5800., wb_red: 1., wb_blue: -3., highlights: -2., shadows: 3., color: 4., clarity: -3.,
            grain_size: "Large".into(), color_chrome: "Strong".into(), blue_chrome: "Weak".into(),
            exposure_hint: "+2/3 to +1 EV (shooting advice)".into(),
            source: "https://fujixweekly.com/2022/12/06/pacific-blues-fujifilm-x-t5-x-trans-v-film-simulation-recipe/".into(),
            ..common.clone() },
        Recipe { name: "Classic B&W".into(), simulation: "Acros+G".into(), white_balance: "Incandescent".into(),
            wb_red: -9., wb_blue: 9., highlights: 3., shadows: 4., sharpness: 1., clarity: 3.,
            grain: "Strong".into(), grain_size: "Large".into(), dynamic_range: "DR200".into(),
            author: common.author, iso_hint: "Auto, up to ISO 12800".into(),
            exposure_hint: "0 to +2/3 EV (shooting advice)".into(),
            source: "https://fujixweekly.com/2025/08/03/classic-bw-film-simulation-recipe/".into(), ..Default::default() },
    ]
}

pub fn load_library(path: &Path) -> Result<Vec<Recipe>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    ensure!(
        fs::metadata(path)?.len() <= 2_000_000,
        "Recipe library is too large."
    );
    let recipes: Vec<Recipe> = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        recipes.len() <= 1000,
        "The recipe library contains too many entries."
    );
    for recipe in &recipes {
        recipe.validate()?;
    }
    Ok(recipes)
}

pub fn save_library(path: &Path, recipes: &[Recipe]) -> Result<()> {
    for recipe in recipes {
        recipe.validate()?;
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(temporary.as_file_mut(), recipes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_settings_and_nonfinite_values() {
        assert!(serde_json::from_str::<Recipe>(r#"{"film":"something"}"#).is_err());
        assert!(
            Recipe {
                highlights: f32::NAN,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn library_roundtrips_and_invalid_save_preserves_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("recipes.json");
        let recipes = builtins();
        save_library(&path, &recipes).unwrap();
        assert_eq!(load_library(&path).unwrap(), recipes);
        let bad = Recipe {
            wb_red: 999.,
            ..Default::default()
        };
        assert!(save_library(&path, &[bad]).is_err());
        assert_eq!(load_library(&path).unwrap(), recipes);
    }
}
