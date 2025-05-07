use std::path::{Path, PathBuf};

use crate::constants::DEFAULT_JSON;
use dirs::home_dir;
use eyre::Result;
use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io;

pub struct ConfFile {
    contents: String,
}

impl ConfFile {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let contents = fs::read_to_string(&path)?;
        Ok(Self { contents })
    }

    pub fn read(&self) -> String {
        self.contents.to_string()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SystemConfig {
    helper_path: String,
}

impl SystemConfig {
    pub fn from_file() -> Result<Self> {
        let mut fig = Figment::new();
        // Prioritize configuration in local, as semantically that is the users config
        if Path::new("/usr/local/etc/cutekit/config.toml").exists() {
            fig = fig.merge(Toml::file_exact("/usr/local/etc/cutekit/config.toml"));
            tracing::info!("using configuration file found at /usr/local/etc/cutekit/config.toml");
        // Try the configuration location of the distro
        } else if Path::new("/etc/cutekit/config.toml").exists() {
            fig = fig.merge(Toml::file_exact("/etc/cutekit/config.toml"));
            tracing::info!("using configuration file found at /etc/cutekit/config.toml");
        // Fall back to default
        } else {
            fig = fig.merge(Serialized::defaults(Self::default()));
            tracing::info!("no configuration file found, using default configuration instead");
        }
        Ok(fig.extract()?)
    }

    pub fn get_helper_path(&self) -> &str {
        &self.helper_path
    }
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            helper_path: env!("POLKIT_AGENT_HELPER_PATH").into(),
        }
    }
}

pub fn files_init() -> io::Result<()> {
    if let Some(home) = home_dir() {
        let cutekit = home.join(".config/cutekit");
        let conf = cutekit.join("config.json");
        if !cutekit.exists() || !cutekit.is_dir() {
            println!("Creating {} directory.", cutekit.to_string_lossy());
            if !PathBuf::from(".config").exists() {
                fs::create_dir_all(cutekit)?;
            } else {
                fs::create_dir(cutekit)?;
            }
        }

        if !conf.exists() || !conf.is_file() {
            println!("Creating {}", conf.to_string_lossy());
            File::create(conf.clone())?;
            fs::write(conf, DEFAULT_JSON)?;
        }
    }
    Ok(())
}
