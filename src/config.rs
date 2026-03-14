use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    pub paths: PathsConfig,
    pub ocr: OcrConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PathsConfig {
    pub screenshots: PathBuf,
    pub database: PathBuf,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OcrConfig {
    pub language: String,
}

impl Default for Config {
    fn default() -> Self {
        let screenshots = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Desktop");

        let database = dirs::data_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".local/share")
            })
            .join("shotext")
            .join("index.db");

        Config {
            paths: PathsConfig {
                screenshots,
                database,
            },
            ocr: OcrConfig {
                language: "eng".to_string(),
            },
        }
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[paths]")?;
        writeln!(f, "screenshots = {:?}", self.paths.screenshots.display())?;
        writeln!(f, "database    = {:?}", self.paths.database.display())?;
        writeln!(f)?;
        writeln!(f, "[ocr]")?;
        write!(f, "language    = {:?}", self.ocr.language)
    }
}

/// Returns the path to the config file.
pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .expect("Could not find config directory")
        .join("shotext")
        .join("config.toml")
}

/// Loads the config from disk, creating a default one if it doesn't exist.
pub fn load() -> Result<Config, std::io::Error> {
    let config_path = config_path();
    let config_dir = config_path.parent().expect("Config path has no parent");

    // Create config directory if it doesn't exist.
    fs::create_dir_all(config_dir)?;

    // If the config file doesn't exist, create it with default values.
    if !config_path.exists() {
        let default_config = Config::default();
        let toml_string =
            toml::to_string_pretty(&default_config).expect("Could not serialize default config");
        fs::write(&config_path, toml_string)?;
    }

    // Read the config file from disk.
    let toml_content = fs::read_to_string(&config_path)?;
    let config: Config = toml::from_str(&toml_content).expect("Could not deserialize config file");

    Ok(config)
}
