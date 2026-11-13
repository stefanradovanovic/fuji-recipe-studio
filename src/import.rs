use crate::recipe::{Recipe, STRENGTHS, WHITE_BALANCES, normalize, simulation};
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use scraper::{Html, Selector};
use std::{
    collections::HashSet,
    io::Read,
    net::{IpAddr, ToSocketAddrs},
    time::Duration,
};
use url::Url;

#[derive(Debug)]
pub struct Imported {
    pub recipe: Recipe,
    pub warnings: Vec<String>,
}

fn strength(value: &str) -> Result<String> {
    STRENGTHS
        .iter()
        .find(|s| value.to_lowercase().contains(&s.to_lowercase()))
        .map(|s| s.to_string())
        .context("Expected Off, Weak or Strong.")
}

fn parse_shift(value: &str, recipe: &mut Recipe) -> Result<bool> {
    let mut found = false;
    for (word, channel) in [("Red", &mut recipe.wb_red), ("Blue", &mut recipe.wb_blue)] {
        let first = Regex::new(&format!(r"(?i)([+-]?\d+)\s*{word}\b"))?;
        let last = Regex::new(&format!(
            r"(?i)\b(?:{word}|{})\s*:?\s*([+-]?\d+)",
            &word[..1]
        ))?;
        if let Some(cap) = first.captures(value).or_else(|| last.captures(value)) {
            *channel = cap[1].parse()?;
            found = true;
        }
    }
    Ok(found)
}

pub fn parse_text(text: &str, name: &str, source: &str) -> Result<Imported> {
    ensure!(text.len() <= 200_000, "Recipe text is too long.");
    let text = text
        .replace(['−', '–'], "-")
        .replace('：', ":")
        .replace('\u{a0}', " ")
        .replace("**", "");
    let mut recipe = Recipe {
        name: if name.trim().is_empty() {
            "Imported recipe".into()
        } else {
            name.trim().into()
        },
        source: source.trim().into(),
        ..Default::default()
    };
    let mut found_sim = false;
    let mut seen = HashSet::new();
    let mut warnings = Vec::new();
    let number = Regex::new(r"^([+-]?\d+(?:\.\d+)?)")?;
    let temperature = Regex::new(r"(?i)\b(\d{4,5})\s*K\b")?;
    for line in text
        .lines()
        .map(|l| l.trim().trim_start_matches(['*', '•', '#']).trim())
        .filter(|l| !l.is_empty())
    {
        let Some((label, value)) = line.split_once(':') else {
            if !found_sim {
                if let Ok(sim) = simulation(line) {
                    recipe.simulation = sim;
                    found_sim = true;
                } else if recipe.name == "Imported recipe" && line.len() < 100 && line.len() > 3 {
                    recipe.name = line.split(" — ").next().unwrap_or(line).into();
                }
            }
            continue;
        };
        let key = normalize(label);
        let value = value.trim();
        if !seen.insert(key.clone()) {
            continue;
        }
        match key.as_str() {
            "filmsimulation" | "simulation" => {
                recipe.simulation = simulation(value)?;
                found_sim = true;
            }
            "name" | "recipename" if name.trim().is_empty() => recipe.name = value.into(),
            "highlight" | "highlights" | "highlighttone" | "shadow" | "shadows" | "shadowtone"
            | "color" | "colour" | "sharpness" | "clarity" | "highisonr" | "noisereduction" => {
                let n: f32 = number
                    .captures(value)
                    .with_context(|| format!("Cannot read {label}: {value}"))?[1]
                    .parse()?;
                match key.as_str() {
                    "highlight" | "highlights" | "highlighttone" => recipe.highlights = n,
                    "shadow" | "shadows" | "shadowtone" => recipe.shadows = n,
                    "color" | "colour" => recipe.color = n,
                    "sharpness" => recipe.sharpness = n,
                    "clarity" => recipe.clarity = n,
                    _ => recipe.noise_reduction = n,
                }
            }
            "whitebalance" | "wb" => {
                if let Some(cap) = temperature.captures(value) {
                    recipe.white_balance = "Kelvin".into();
                    recipe.kelvin = cap[1].parse()?;
                } else {
                    let wb = value.split(',').next().unwrap_or(value).trim();
                    recipe.white_balance = if let Some(v) = WHITE_BALANCES
                        .iter()
                        .find(|v| normalize(v) == normalize(wb))
                    {
                        v.to_string()
                    } else {
                        match normalize(wb).as_str() {
                            "sunny" | "fine" => "Daylight".into(),
                            "tungsten" => "Incandescent".into(),
                            "fluorescentlight1" => "Fluorescent 1".into(),
                            "fluorescentlight2" => "Fluorescent 2".into(),
                            "fluorescentlight3" => "Fluorescent 3".into(),
                            v if v.starts_with("auto") => {
                                warnings.push(
                                    "Auto WB variants use the recorded camera white balance."
                                        .into(),
                                );
                                "Auto".into()
                            }
                            _ => bail!("Cannot read white balance: {wb}"),
                        }
                    };
                }
                parse_shift(value, &mut recipe)?;
            }
            "wbshift" | "whitebalanceshift" => {
                ensure!(
                    parse_shift(value, &mut recipe)?,
                    "Write WB shift as +2 Red, -3 Blue or R:2 B:-3."
                );
            }
            "graineffect" | "grain" => {
                recipe.grain = strength(value)?;
                if value.to_lowercase().contains("large") {
                    recipe.grain_size = "Large".into();
                }
            }
            "grainsize" => {
                recipe.grain_size = if value.to_lowercase().contains("large") {
                    "Large"
                } else {
                    "Small"
                }
                .into()
            }
            "colorchromeeffect" | "colorchrome" => recipe.color_chrome = strength(value)?,
            "colorchromefxblue" | "colorchromeblue" | "fxblue" => {
                recipe.blue_chrome = strength(value)?
            }
            "dynamicrange" | "dr" => {
                recipe.dynamic_range = if value.to_lowercase().starts_with("auto") {
                    "Auto".into()
                } else if value.contains("400") {
                    "DR400".into()
                } else if value.contains("200") {
                    "DR200".into()
                } else if value.contains("100") {
                    "DR100".into()
                } else {
                    bail!("Cannot read dynamic range: {value}");
                };
            }
            "monochromaticcolor" | "monochromaticcolortoning" => {
                for (label, channel) in [("WC", &mut recipe.mono_wc), ("MG", &mut recipe.mono_mg)] {
                    if let Some(cap) =
                        Regex::new(&format!(r"(?i){label}\s*:?\s*([+-]?\d+)"))?.captures(value)
                    {
                        *channel = cap[1].parse()?;
                    }
                }
            }
            "exposurecompensation" | "exposure" => recipe.exposure_hint = value.into(),
            "iso" => recipe.iso_hint = value.into(),
            "author" | "creator" => recipe.author = value.into(),
            "source" | "url" if source.trim().is_empty() => recipe.source = value.into(),
            _ if label.len() < 45 && found_sim => warnings.push(format!(
                "Not applied: {label}: {}",
                value.chars().take(90).collect::<String>()
            )),
            _ => {}
        }
    }
    ensure!(
        found_sim,
        "No film simulation found. Paste a specific recipe's settings, such as 'Film Simulation: Classic Chrome'. A recipe index page is not a recipe."
    );
    if !seen.contains("whitebalance") && !seen.contains("wb") {
        warnings.push("White balance was omitted; using As shot.".into());
    }
    if !recipe.exposure_hint.is_empty() {
        warnings.push("Exposure advice is saved as a reference; it is not automatically added to your RAW exposure.".into());
    }
    recipe.validate()?;
    Ok(Imported { recipe, warnings })
}

fn validate_url(url: &Url) -> Result<()> {
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none(),
        "Use a public http or https recipe URL."
    );
    let host = url.host_str().context("URL has no host.")?;
    let addresses: Vec<_> = (host, url.port_or_known_default().unwrap_or(443))
        .to_socket_addrs()?
        .collect();
    ensure!(
        !addresses.is_empty(),
        "Website address could not be resolved."
    );
    for addr in addresses {
        let public = match addr.ip() {
            IpAddr::V4(ip) => {
                !(ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_unspecified()
                    || ip.is_broadcast())
            }
            IpAddr::V6(ip) => {
                !(ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || ip.to_ipv4_mapped().is_some())
            }
        };
        ensure!(public, "Recipe import accepts public websites only.");
    }
    Ok(())
}

pub fn fetch_recipe(input: &str) -> Result<Imported> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .redirects(0)
        .build();
    let mut url =
        Url::parse(input.trim()).context("Paste a full recipe URL, starting with https://.")?;
    for _ in 0..6 {
        validate_url(&url)?;
        let response = agent.get(url.as_str()).set("User-Agent", "FujiRecipeStudio/0.1 (personal recipe importer)").call()
            .context("Could not fetch this page. You can still copy its recipe settings into the text box.")?;
        if (300..400).contains(&response.status()) {
            url = url.join(
                response
                    .header("Location")
                    .context("Invalid website redirect.")?,
            )?;
            continue;
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(2_000_001)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 2_000_000,
            "Page is too large. Paste the settings instead."
        );
        let document = Html::parse_document(&String::from_utf8_lossy(&bytes));
        let title = document
            .select(&Selector::parse("h1.entry-title").unwrap())
            .next()
            .or_else(|| document.select(&Selector::parse("h1").unwrap()).next())
            .map(|e| e.text().collect::<String>())
            .unwrap_or_default();
        let name = title
            .split(" — ")
            .next()
            .unwrap_or(&title)
            .replace(" Film Simulation Recipe", "");
        let content = document
            .select(&Selector::parse(".entry-content").unwrap())
            .next()
            .or_else(|| document.select(&Selector::parse("article").unwrap()).next())
            .or_else(|| document.select(&Selector::parse("body").unwrap()).next())
            .context("Page contains no readable recipe.")?;
        let mut text = String::new();
        for node in content.descendants() {
            match node.value() {
                scraper::node::Node::Element(el)
                    if ["br", "p", "div", "h1", "h2", "h3", "li", "tr"].contains(&el.name()) =>
                {
                    text.push('\n')
                }
                scraper::node::Node::Text(t) => {
                    let hidden = node.ancestors().any(|n| matches!(n.value(), scraper::node::Node::Element(e) if ["script", "style", "nav"].contains(&e.name())));
                    if !hidden {
                        text.push_str(t);
                    }
                }
                _ => {}
            }
        }
        let text = text.split("Example photographs").next().unwrap_or(&text);
        let start = text
            .lines()
            .position(|l| normalize(l.split(':').next().unwrap_or("").trim()) == "filmsimulation");
        let settings = if let Some(start) = start {
            text.lines().skip(start).collect::<Vec<_>>().join("\n")
        } else {
            text.into()
        };
        let mut imported = parse_text(&settings, &name, url.as_str())?;
        if url
            .host_str()
            .is_some_and(|h| h == "fujixweekly.com" || h == "www.fujixweekly.com")
        {
            imported.recipe.author = "Ritchie Roesch / Fuji X Weekly".into();
        }
        return Ok(imported);
    }
    bail!("Too many website redirects. Paste the recipe settings instead.")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_half_steps_shifts_and_preserves_shooting_advice() {
        let imported = parse_text("Film Simulation: Classic Chrome\nWhite Balance: 5200K, +1 Red & −6 Blue\nHighlight: -1.5\nShadow: +0.5\nGrain Effect: Strong, Small\nExposure Compensation: +1/3 to +1\nISO: Auto, up to 6400", "Test", "").unwrap();
        assert_eq!(imported.recipe.wb_blue, -6.);
        assert_eq!(imported.recipe.kelvin, 5200.);
        assert_eq!(imported.recipe.highlights, -1.5);
        assert_eq!(imported.recipe.shadows, 0.5);
        assert_eq!(imported.recipe.exposure_hint, "+1/3 to +1");
    }
    #[test]
    fn rejects_empty_index_unknown_simulation_and_out_of_range() {
        assert!(parse_text("Film Simulation Recipes\nChoose your sensor", "", "").is_err());
        assert!(parse_text("Film Simulation: Made Up", "", "").is_err());
        assert!(parse_text("Film Simulation: Provia\nColor: 99", "", "").is_err());
    }
    #[test]
    fn handles_markdown_and_separate_wb_shift() {
        let value = parse_text(
            "**Film Simulation:** Acros+G\nWhite Balance: Auto\nWB Shift: R:2 B:-3",
            "",
            "",
        )
        .unwrap();
        assert_eq!((value.recipe.wb_red, value.recipe.wb_blue), (2., -3.));
    }
}
