use std::collections::HashMap;
use std::path::Path;
use async_trait::async_trait;
use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginCategory {
    Recon,
    Scan,
    Exploit,
    Utility,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub description: String,
    pub version: String,
    pub category: PluginCategory,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PluginResult {
    pub status: PluginStatus,
    pub message: String,
    pub data: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PluginStatus {
    Success,
    Error,
    Partial,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;
    
    async fn setup(&mut self) -> Result<()>;
    
    async fn execute(
        &mut self, 
        target: &str, 
        options: Option<HashMap<String, Value>>
    ) -> Result<PluginResult>;
    
    async fn cleanup(&mut self) -> Result<()>;
}

pub struct PluginManager {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub async fn load_plugins(&mut self, plugin_dir: &Path) -> Result<()> {
        // Dynamically load plugins from directory
        // Implementation depends on your plugin loading strategy
        Ok(())
    }

    pub async fn run_plugin(
        &mut self, 
        plugin_name: &str, 
        target: &str, 
        options: Option<HashMap<String, Value>>
    ) -> Result<PluginResult> {
        let plugin = self.plugins.get_mut(plugin_name)
            .context("Plugin not found")?;
        
        plugin.setup().await?;
        let result = plugin.execute(target, options).await;
        plugin.cleanup().await?;
        
        result
    }
}
