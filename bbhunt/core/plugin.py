#!/usr/bin/env python3
# core/plugin.py - Plugin system implementation

import importlib
import inspect
import os
import logging
import pkgutil
from typing import Dict, Type, List, Any, Optional

class Plugin:
    """Base class for all plugins."""
    
    __plugin_name__ = "base_plugin"
    __plugin_description__ = "Base plugin class"
    __plugin_version__ = "1.0.0"
    __plugin_category__ = "core"
    __plugin_dependencies__ = []
    __plugin_resources__ = {
        "memory": "100MB",
        "cpu": 0.5,
        "disk": "10MB",
        "network": False
    }
    
    def __init__(self):
        """Initialize the plugin."""
        self.logger = logging.getLogger(f"bbhunt.plugin.{self.__plugin_name__}")
    
    def setup(self):
        """Set up the plugin. Override in subclasses."""
        pass
    
    def execute(self, target: str, options: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """
        Execute the plugin.
        
        Args:
            target: Target to run against
            options: Plugin options
            
        Returns:
            Dict with status and results
        """
        raise NotImplementedError("Plugin must implement execute method")
    
    def cleanup(self):
        """Clean up after plugin execution. Override in subclasses."""
        pass
    
    @classmethod
    def cli_options(cls) -> List[Dict[str, Any]]:
        """
        Define CLI options for the plugin.
        
        Returns:
            List of option dictionaries for Click
        """
        return []
    
    @classmethod
    def interactive_options(cls) -> List[Dict[str, Any]]:
        """
        Define interactive prompts for the plugin.
        
        Returns:
            List of prompt dictionaries for prompt_toolkit
        """
        return []

def discover_plugins() -> Dict[str, Dict[str, Type[Plugin]]]:
    """
    Discover all available plugins.
    
    Returns:
        Dict of plugin categories -> Dict of plugin names -> plugin classes
    """
    plugins = {}
    
    # Get the plugins package
    plugins_dir = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'plugins')
    
    # Walk through all plugin directories
    for category in os.listdir(plugins_dir):
        category_dir = os.path.join(plugins_dir, category)
        
        if not os.path.isdir(category_dir) or category.startswith('__'):
            continue
        
        # Initialize category dict
        plugins[category] = {}
        
        # Import all modules in this category
        package_name = f"bbhunt.plugins.{category}"
        
        # Use pkgutil to find all modules
        for _, module_name, is_pkg in pkgutil.iter_modules([category_dir]):
            if is_pkg:
                continue
                
            try:
                # Import the module
                module = importlib.import_module(f"{package_name}.{module_name}")
                
                # Find plugin classes in the module
                for name, obj in inspect.getmembers(module):
                    if (inspect.isclass(obj) and 
                        issubclass(obj, Plugin) and 
                        obj != Plugin and 
                        hasattr(obj, '__plugin_name__')):
                        
                        plugin_name = obj.__plugin_name__
                        plugins[category][plugin_name] = obj
            except Exception as e:
                logging.error(f"Error loading plugin {module_name}: {str(e)}")
    
    return plugins

def get_all_plugins() -> Dict[str, Dict[str, Type[Plugin]]]:
    """
    Get all available plugins.
    
    Returns:
        Dict of plugin categories -> Dict of plugin names -> plugin classes
    """
    return discover_plugins()

def get_plugin(plugin_name: str) -> Optional[Type[Plugin]]:
    """
    Get a plugin by name.
    
    Args:
        plugin_name: Name of the plugin
        
    Returns:
        Plugin class or None if not found
    """
    plugins = discover_plugins()
    
    for category, category_plugins in plugins.items():
        if plugin_name in category_plugins:
            return category_plugins[plugin_name]
    
    return None
