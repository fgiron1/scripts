use std::collections::HashMap;
use anyhow::{Result, Context};
use async_trait::async_trait;
use serde_json::Value;
use tracing::{info, error};

use crate::core::plugin::{Plugin, PluginMetadata, PluginCategory, PluginResult, PluginStatus};
use crate::utils::http::HttpClient;

#[derive(Default)]
pub struct SubdomainEnumPlugin {
    metadata: PluginMetadata,
    http_client: HttpClient,
    tools: Vec<SubdomainTool>,
}

#[derive(Debug)]
struct SubdomainTool {
    name: String,
    command: String,
    passive: bool,
}

#[async_trait]
impl Plugin for SubdomainEnumPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    async fn setup(&mut self) -> Result<()> {
        self.metadata = PluginMetadata {
            name: "subdomain_enum".to_string(),
            description: "Enumerate subdomains using various techniques".to_string(),
            version: "0.1.0".to_string(),
            category: PluginCategory::Recon,
        };

        // Initialize subdomain tools
        self.tools = vec![
            SubdomainTool {
                name: "subfinder".to_string(),
                command: "subfinder -d {} -o {}".to_string(),
                passive: false,
            },
            SubdomainTool {
                name: "amass".to_string(),
                command: "amass enum -d {} -o {}".to_string(),
                passive: true,
            },
        ];

        Ok(())
    }

    async fn execute(
        &mut self, 
        target: &str, 
        options: Option<HashMap<String, Value>>
    ) -> Result<PluginResult> {
        let passive_only = options
            .and_then(|opts| opts.get("passive_only").and_then(|v| v.as_bool()))
            .unwrap_or(false);

        let mut all_subdomains = Vec::new();

        for tool in &self.tools {
            // Skip tools based on passive mode
            if passive_only && !tool.passive {
                continue;
            }

            match self.run_subdomain_tool(target, &tool.name, &tool.command).await {
                Ok(subdomains) => {
                    info!("Found {} subdomains with {}", subdomains.len(), tool.name);
                    all_subdomains.extend(subdomains);
                }
                Err(e) => {
                    error!("Error running {}: {}", tool.name, e);
                }
            }
        }

        // Deduplicate subdomains
        let unique_subdomains: Vec<String> = all_subdomains
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Verify live subdomains
        let live_subdomains = self.verify_live_subdomains(&unique_subdomains).await?;

        let mut result_data = HashMap::new();
        result_data.insert("total_subdomains".to_string(), Value::Number(unique_subdomains.len().into()));
        result_data.insert("live_subdomains".to_string(), Value::Number(live_subdomains.len().into()));
        result_data.insert("subdomains".to_string(), Value::Array(
            unique_subdomains.into_iter().map(Value::String).collect()
        ));

        Ok(PluginResult {
            status: PluginStatus::Success,
            message: format!("Found {} total subdomains", unique_subdomains.len()),
            data: result_data,
        })
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Clean up any temporary files or resources
        Ok(())
    }
}

impl SubdomainEnumPlugin {
    async fn run_subdomain_tool(
        &self, 
        target: &str, 
        tool_name: &str, 
        command_template: &str
    ) -> Result<Vec<String>> {
        // Create temporary output file
        let output_file = tempfile::NamedTempFile::new()?;
        
        // Format command
        let command = command_template
            .replace("{}", target)
            .replace("{}", output_file.path().to_str().unwrap());

        // Execute command
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .output()
            .await
            .context("Failed to execute subdomain tool")?;

        // Read results
        let subdomains = std::fs::read_to_string(output_file.path())
            .context("Failed to read subdomain results")?
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(subdomains)
    }

    async fn verify_live_subdomains(&self, subdomains: &[String]) -> Result<Vec<String>> {
        let client = reqwest::Client::new();
        
        let live_checks = subdomains.iter().map(|subdomain| async move {
            let url = format!("https://{}", subdomain);
            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => Some(subdomain.clone()),
                _ => None,
            }
        });

        let live_subdomains = futures::future::join_all(live_checks)
            .await
            .into_iter()
            .flatten()
            .collect();

        Ok(live_subdomains)
    }
}

// Register the plugin
crate::register_plugin!(SubdomainEnumPlugin);
