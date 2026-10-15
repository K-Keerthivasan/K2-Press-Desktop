//! `config.yaml`, read as the web app reads it.

use photocraft_press::Brand;
use serde_json::Value;

use crate::home::Home;
use crate::{Result, StudioError};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    /// The whole file as JSON (unknown sections kept for whoever needs them).
    pub raw: Value,
    pub brands: Vec<Brand>,
    pub active_brand: String,
}

impl Config {
    pub fn load(home: &Home) -> Result<Config> {
        let path = home.config_path();
        let text = std::fs::read_to_string(&path).map_err(|e| StudioError(format!("could not read {}: {e}", path.display())))?;
        Config::parse(&text, home)
    }

    pub fn parse(text: &str, home: &Home) -> Result<Config> {
        let yaml: serde_yaml::Value = serde_yaml::from_str(text).map_err(|e| StudioError(format!("config.yaml is not valid YAML: {e}")))?;
        let raw: Value = serde_json::to_value(yaml).map_err(|e| StudioError(format!("config.yaml: {e}")))?;
        let mut brands = Vec::new();
        if let Some(map) = raw.get("brands").and_then(Value::as_object) {
            for (k, v) in map {
                let mut b = Brand::from_value(k, v);
                if !b.logo_path.trim().is_empty() {
                    let p = home.resolve(b.logo_path.trim());
                    if p.is_file() {
                        b.logo = Some(p.to_string_lossy().replace('\\', "/"));
                    }
                }
                brands.push(b);
            }
        }
        let active_brand = raw.get("active_brand").and_then(Value::as_str).unwrap_or("").to_string();
        Ok(Config { raw, brands, active_brand })
    }

    pub fn brand(&self, key: &str) -> Option<&Brand> {
        self.brands.iter().find(|b| b.key == key)
    }

    /// Value at a dotted path (`schedule.times`).
    pub fn at(&self, path: &str) -> Option<&Value> {
        path.split('.').try_fold(&self.raw, |v, k| v.get(k))
    }

    pub fn str_at(&self, path: &str) -> Option<String> {
        self.at(path).and_then(Value::as_str).map(str::to_string)
    }

    /// The brand as `press.build` takes it (with the resolved logo).
    pub fn brand_json(b: &Brand) -> Value {
        let mut v = serde_json::to_value(b).unwrap_or(Value::Null);
        if let (Some(o), Some(l)) = (v.as_object_mut(), &b.logo) {
            o.insert("logo".into(), Value::String(l.clone()));
        }
        v
    }
}
