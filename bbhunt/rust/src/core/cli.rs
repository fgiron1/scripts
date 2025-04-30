use std::collections::HashMap;
use clap::{Parser, Subcommand};
use anyhow::{Result, Context};
use dialoguer::{theme::ColorfulTheme, Input, Confirm};
use serde_json::Value;

use crate::core::{
    config::BBHuntConfig,
    plugin::{PluginManager, PluginResult},
    resource_manager::ResourceManager,
};

#[derive(Parser)]
#[command(name = "bbhunt")]
#[command(about = "A modular bug bounty hunting framework")]
pub struct BBHuntCli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new target for reconnaissance
    Target {
        #[arg(help = "Domain or URL to add")]
        domain: String,
    },

    /// Run a specific plugin
    Run {
        #[arg(help = "Plugin name to run")]
        plugin: String,

        #[arg(help = "Target domain")]
        target: String,

        #[arg(long, help = "JSON-formatted options")]
        options: Option<String>,
    },

    /// List available plugins
    Plugins {
        #[arg(long, help = "Filter by category")]
        category: Option<String>,
    },

    /// Show system resource usage
    Resources,

    /// Start an interactive session
    Interactive,
}

impl BBHuntCli {
    pub fn new() -> Self {
        Self::parse()
    }

    pub async fn run(&self) -> Result<()> {
        // Initialize core components
        let mut config = BBHuntConfig::load(None)?;
        let resource_manager = ResourceManager::new();
        let mut plugin_manager = PluginManager::new();

        // Load plugins
        plugin_manager.load_plugins(&config.global.config_dir.join("plugins")).await?;

        match &self.command {
            Some(Commands::Target { domain }) => {
                config.add_target(domain)?;
                config.save(&config.global.config_dir.join("config.toml"))?;
                println!("Added target: {}", domain);
            }
            Some(Commands::Run { plugin, target, options }) => {
                // Parse options from JSON if provided
                let parsed_options = options
                    .as_ref()
                    .map(|opts| serde_json::from_str(opts).context("Invalid JSON options"))
                    .transpose()?;

                let result = plugin_manager.run_plugin(plugin, target, parsed_options).await?;
                self.display_plugin_result(&result);
            }
            Some(Commands::Plugins { category }) => {
                // Implement plugin listing logic
                println!("Available plugins...");
            }
            Some(Commands::Resources) => {
                let usage = resource_manager.get_resource_usage().await?;
                println!("{:#?}", usage);
            }
            Some(Commands::Interactive) => {
                self.start_interactive_session(&mut config, &mut plugin_manager).await?;
            }
            None => {
                self.show_help();
            }
        }

        Ok(())
    }

    fn display_plugin_result(&self, result: &PluginResult) {
        match result.status {
            PluginResult::Success => {
                println!("Plugin executed successfully!");
                println!("Message: {}", result.message);
                // Pretty print result data
                println!("Data: {:#?}", result.data);
            }
            PluginResult::Error => {
                eprintln!("Plugin execution failed!");
                eprintln!("Error: {}", result.message);
            }
            PluginResult::Partial => {
                println!("Plugin partially completed.");
                println!("Message: {}", result.message);
            }
        }
    }

    async fn start_interactive_session(
        &self, 
        config: &mut BBHuntConfig, 
        plugin_manager: &mut PluginManager
    ) -> Result<()> {
        loop {
            let action: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("bbhunt")
                .interact_text()?;

            match action.as_str() {
                "exit" | "quit" => break,
                "help" => self.show_help(),
                _ => {
                    println!("Unknown command. Type 'help' for assistance.");
                }
            }
        }
        Ok(())
    }

    fn show_help(&self) {
        println!("BBHunt - Bug Bounty Hunting Framework");
        println!("Available commands:");
        println!("  target <domain>     Add a new target");
        println!("  run <plugin> <target>  Run a specific plugin");
        println!("  plugins             List available plugins");
        println!("  resources           Show system resource usage");
        println!("  interactive         Start interactive session");
        println!("  help                Show this help message");
    }
}
