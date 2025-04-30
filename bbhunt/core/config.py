#!/usr/bin/env python3
# core/config.py - Configuration management

import os
import yaml
from typing import Dict, Any, Optional

class Config:
    """Configuration management for the framework."""
    
    def __init__(self, config_dir: str = 'config'):
        """
        Initialize configuration.
        
        Args:
            config_dir: Directory for configuration files
        """
        self.config_dir = config_dir
        self.main_config_file = os.path.join(config_dir, 'bbhunt.yaml')
        self.plugin_config_dir = os.path.join(config_dir, 'plugins')
        
        # Create directories if they don't exist
        os.makedirs(self.config_dir, exist_ok=True)
        os.makedirs(self.plugin_config_dir, exist_ok=True)
        
        # Load main configuration
        self.config = self._load_config(self.main_config_file)
        
        # Load plugin configurations
        self.plugin_configs = {}
        self._load_plugin_configs()
    
    def _load_config(self, config_file: str) -> Dict[str, Any]:
        """
        Load configuration from file.
        
        Args:
            config_file: Path to configuration file
            
        Returns:
            Configuration dict
        """
        if os.path.exists(config_file):
            try:
                with open(config_file, 'r') as f:
                    return yaml.safe_load(f) or {}
            except Exception as e:
                print(f"Error loading config from {config_file}: {str(e)}")
        
        return {}
    
    def _load_plugin_configs(self):
        """Load all plugin configurations."""
        if not os.path.exists(self.plugin_config_dir):
            return
        
        for filename in os.listdir(self.plugin_config_dir):
            if filename.endswith('.yaml'):
                plugin_name = os.path.splitext(filename)[0]
                config_file = os.path.join(self.plugin_config_dir, filename)
                self.plugin_configs[plugin_name] = self._load_config(config_file)
    
    def get(self, key: str, default: Any = None) -> Any:
        """
        Get a configuration value.
        
        Args:
            key: Configuration key
            default: Default value if key not found
            
        Returns:
            Configuration value
        """
        return self.config.get(key, default)
    
    def set(self, key: str, value: Any) -> None:
        """
        Set a configuration value.
        
        Args:
            key: Configuration key
            value: Configuration value
        """
        self.config[key] = value
        self._save_config()
    
    def get_plugin_config(self, plugin_name: str) -> Dict[str, Any]:
        """
        Get configuration for a plugin.
        
        Args:
            plugin_name: Name of the plugin
            
        Returns:
            Plugin configuration dict
        """
        return self.plugin_configs.get(plugin_name, {})
    
    def set_plugin_config(self, plugin_name: str, config: Dict[str, Any]) -> None:
        """
        Set configuration for a plugin.
        
        Args:
            plugin_name: Name of the plugin
            config: Plugin configuration
        """
        self.plugin_configs[plugin_name] = config
        self._save_plugin_config(plugin_name)
    
    def _save_config(self) -> None:
        """Save main configuration to file."""
        os.makedirs(os.path.dirname(self.main_config_file), exist_ok=True)
        
        try:
            with open(self.main_config_file, 'w') as f:
                yaml.dump(self.config, f, default_flow_style=False)
        except Exception as e:
            print(f"Error saving config to {self.main_config_file}: {str(e)}")
    
    def _save_plugin_config(self, plugin_name: str) -> None:
        """
        Save plugin configuration to file.
        
        Args:
            plugin_name: Name of the plugin
        """
        config_file = os.path.join(self.plugin_config_dir, f"{plugin_name}.yaml")
        os.makedirs(os.path.dirname(config_file), exist_ok=True)
        
        try:
            with open(config_file, 'w') as f:
                yaml.dump(self.plugin_configs[plugin_name], f, default_flow_style=False)
        except Exception as e:
            print(f"Error saving plugin config to {config_file}: {str(e)}")
    
    def reset(self) -> None:
        """Reset configuration to defaults."""
        self.config = {}
        self._save_config()
