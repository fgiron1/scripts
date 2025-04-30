use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use config::{Config, ConfigError, File, FileFormat};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BBHuntConfig {
    pub global: GlobalConfig,
    pub plugins: HashMap<String, PluginConfig>,
    pub targets: HashMap<String, TargetConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GlobalConfig {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub max_memory: usize,
    pub max_cpu: usize,
    pub user_agent: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PluginConfig {
    pub enabled: bool,
    pub default_options: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TargetConfig {
    pub domain: String,
    pub scope: Vec<String>,
    pub added_at: String,
    pub notes: Option<String>,
}

impl BBHuntConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let mut config_builder = Config::builder();

        // Default configuration
        config_builder = config_builder.add_source(
            config::File::from_str(
                include_str!("../../config/default.toml"), 
                FileFormat::Toml
            )
        );

        // User-provided configuration
        if let Some(path) = config_path {
            config_builder = config_builder.add_source(File::from(path));
        }

        // Environment variables
        config_builder = config_builder.add_source(
            config::Environment::with_prefix("BBHUNT")
        );

        // Build and parse configuration
        let config: BBHuntConfig = config_builder
            .build()?
            .try_deserialize()
            .context("Failed to load configuration")?;

        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let config_str = toml::to_string_pretty(self)
            .context("Failed to serialize configuration")?;
        
        std::fs::write(path, config_str)
            .context("Failed to write configuration file")?;

        Ok(())
    }

    pub fn add_target(&mut self, domain: &str) -> Result<()> {
        if self.targets.contains_key(domain) {
            return Err(anyhow::anyhow!("Target already exists"));
        }

        let target_config = TargetConfig {
            domain: domain.to_string(),
            scope: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
        };

        self.targets.insert(domain.to_string(), target_config);
        Ok(())
    }

    pub fn get_plugin_config(&self, plugin_name: &str) -> Option<&PluginConfig> {
        self.plugins.get(plugin_name)
    }
}

impl Default for BBHuntConfig {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        
        Self {
            global: GlobalConfig {
                data_dir: home_dir.join(".bbhunt/data"),
                config_dir: home_dir.join(".bbhunt/config"),
                max_memory: 4096, // 4GB
                max_cpu: num_cpus::get(),
                user_agent: "bbhunt/0.1.0".to_string(),
            },
            plugins: HashMap::new(),
            targets: HashMap::new(),
        }
    }
}
