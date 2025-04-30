#!/usr/bin/env python3
# core/cli.py - Command line interface

import click
import os
import json
import yaml
import time
import logging
from datetime import datetime
from typing import Dict, Any, List, Optional
from prompt_toolkit import prompt
from prompt_toolkit.completion import WordCompleter
from prompt_toolkit.history import FileHistory
from prompt_toolkit.styles import Style

from bbhunt.core.plugin import get_all_plugins, get_plugin
from bbhunt.core.resources import ResourceManager, ResourceRequirements
from bbhunt.core.config import Config

# CLI styles
STYLE = Style.from_dict({
    'prompt': '#00aa00 bold',
    'command': '#884444',
    'plugin': '#0000aa',
    'target': '#aa00aa',
})

# Set up logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger("bbhunt.cli")

class BugBountyCLI:
    """Interactive CLI for bug bounty framework."""
    
    def __init__(self):
        """Initialize the CLI."""
        self.resource_manager = ResourceManager()
        self.plugins = get_all_plugins()
        self.config = Config()
        self.current_target = self.config.get('current_target')
        
        # Create history directory
        os.makedirs(os.path.expanduser('~/.bbhunt/history'), exist_ok=True)
        
        # Command history
        self.history = FileHistory(os.path.expanduser('~/.bbhunt/history/commands'))
    
    def show_plugins(self, category: Optional[str] = None) -> None:
        """
        Show available plugins.
        
        Args:
            category: Filter by plugin category
        """
        click.echo(click.style("Available plugins:", fg="green", bold=True))
        
        for plugin_category, plugins in self.plugins.items():
            if category and plugin_category != category:
                continue
                
            click.echo(click.style(f"\n{plugin_category.upper()}:", fg="blue", bold=True))
            
            for plugin_name, plugin_class in plugins.items():
                desc = plugin_class.__plugin_description__
                resources = plugin_class.__plugin_resources__
                memory = resources.get('memory', 'N/A')
                cpu = resources.get('cpu', 'N/A')
                
                click.echo(f"  {click.style(plugin_name, fg='cyan')} - {desc}")
                click.echo(f"    Resources: Memory: {memory}, CPU: {cpu}")
    
    def select_target(self) -> None:
        """Interactive target selection."""
        targets_dir = os.path.join('data', 'targets')
        os.makedirs(targets_dir, exist_ok=True)
        
        targets = [f for f in os.listdir(targets_dir) if os.path.isdir(os.path.join(targets_dir, f))]
        
        if not targets:
            click.echo("No targets found. Let's add one.")
            return self.add_target()
        
        click.echo(click.style("Available targets:", fg="green"))
        for i, target in enumerate(targets, 1):
            click.echo(f"{i}. {target}")
        
        target_completer = WordCompleter(targets)
        response = prompt(
            "Select target (name or number): ",
            completer=target_completer,
            style=STYLE
        )
        
        # Handle numeric selection
        if response.isdigit() and 1 <= int(response) <= len(targets):
            target = targets[int(response) - 1]
        else:
            target = response
            
        if target not in targets:
            click.echo(f"Target '{target}' not found.")
            return
        
        self.current_target = target
        self.config.set('current_target', target)
        click.echo(f"Selected target: {click.style(target, fg='green')}")
    
    def add_target(self) -> None:
        """Add a new target."""
        target = prompt("Enter target domain: ")
        if not target:
            click.echo("Target cannot be empty.")
            return
        
        target_dir = os.path.join('data', 'targets', target)
        if os.path.exists(target_dir):
            click.echo(f"Target '{target}' already exists.")
            return
        
        # Create target directory structure
        os.makedirs(os.path.join(target_dir, 'recon'), exist_ok=True)
        os.makedirs(os.path.join(target_dir, 'scan'), exist_ok=True)
        os.makedirs(os.path.join(target_dir, 'exploit'), exist_ok=True)
        os.makedirs(os.path.join(target_dir, 'report'), exist_ok=True)
        
        # Create target metadata
        metadata = {
            'domain': target,
            'added': datetime.now().isoformat(),
            'notes': '',
            'scope': []
        }
        
        with open(os.path.join(target_dir, 'metadata.yaml'), 'w') as f:
            yaml.dump(metadata, f)
        
        self.current_target = target
        self.config.set('current_target', target)
        
        click.echo(f"Added target: {click.style(target, fg='green')}")
    
    def run_plugin(self, plugin_name: str, args: List[str] = None) -> None:
        """
        Run a plugin.
        
        Args:
            plugin_name: Name of the plugin to run
            args: Additional arguments for the plugin
        """
        # Find plugin
        plugin_class = None
        for category, plugins in self.plugins.items():
            if plugin_name in plugins:
                plugin_class = plugins[plugin_name]
                break
        
        if not plugin_class:
            click.echo(f"Plugin '{plugin_name}' not found.")
            return
        
        # Check if target is selected
        if not self.current_target and plugin_class.__plugin_category__ != 'utility':
            click.echo("No target selected. Please select a target first.")
            return
        
        # Parse plugin options
        options = {}
        
        # Check for -o/--options JSON argument
        if args:
            try:
                option_idx = -1
                for i, arg in enumerate(args):
                    if arg in ['-o', '--options']:
                        option_idx = i
                        break
                
                if option_idx >= 0 and option_idx + 1 < len(args):
                    options_str = args[option_idx + 1]
                    
                    # Remove the options arg and its value from args
                    args = args[:option_idx] + args[option_idx + 2:]
                    
                    # Parse JSON options
                    options = json.loads(options_str)
            except Exception as e:
                click.echo(f"Error parsing options: {str(e)}")
                return
        
        # Get interactive options if not provided
        if not options and hasattr(plugin_class, 'interactive_options') and callable(plugin_class.interactive_options):
            interactive_options = plugin_class.interactive_options()
            
            for option in interactive_options:
                if option['type'] == 'input':
                    value = prompt(f"{option['message']} ", default=option.get('default', ''))
                elif option['type'] == 'confirm':
                    value = prompt(f"{option['message']} (y/n) ", default='y' if option.get('default', False) else 'n')
                    value = value.lower() in ('y', 'yes', 'true')
                else:
                    # Add support for other prompt types as needed
                    value = option.get('default')
                
                options[option['name']] = value
        
        # Check resource requirements
        resources = ResourceRequirements.from_dict(plugin_class.__plugin_resources__)
        can_run, message = self.resource_manager.check_resources(resources)
        
        if not can_run:
            click.echo(f"Insufficient resources: {message}")
            
            # Ask if user wants to run in container
            if self.resource_manager.docker_available:
                run_container = prompt("Run in Docker container instead? (y/n) ", default='y')
                if run_container.lower() in ('y', 'yes'):
                    self._run_in_container(plugin_name, options)
                    return
            
            return
        
        # Instantiate and run plugin
        plugin = plugin_class()
        plugin.setup()
        
        target = self.current_target if plugin_class.__plugin_category__ != 'utility' else None
        
        click.echo(f"Running {click.style(plugin_name, fg='cyan')} on {click.style(target or 'utility mode', fg='green')}...")
        try:
            start_time = time.time()
            result = plugin.execute(target, options)
            end_time = time.time()
            
            if result['status'] == 'success':
                elapsed = end_time - start_time
                click.echo(click.style(f"Plugin completed successfully in {elapsed:.2f} seconds!", fg="green"))
                
                # Show result data if available
                if 'data' in result and result['data']:
                    if isinstance(result['data'], dict):
                        for key, value in result['data'].items():
                            if isinstance(value, list) and len(value) > 10:
                                click.echo(f"  {key}: {len(value)} items")
                            else:
                                click.echo(f"  {key}: {value}")
                    else:
                        click.echo(f"  Result: {result['data']}")
            else:
                click.echo(click.style(f"Plugin failed: {result.get('message', 'Unknown error')}", fg="red"))
        except Exception as e:
            click.echo(click.style(f"Error running plugin: {str(e)}", fg="red"))
        finally:
            plugin.cleanup()
    
    def _run_in_container(self, plugin_name: str, options: Dict[str, Any]) -> None:
        """
        Run a plugin in a Docker container.
        
        Args:
            plugin_name: Name of the plugin to run
            options: Plugin options
        """
        try:
            # Get plugin class to determine resource requirements
            plugin_class = None
            for category, plugins in self.plugins.items():
                if plugin_name in plugins:
                    plugin_class = plugins[plugin_name]
                    break
            
            if not plugin_class:
                click.echo(f"Plugin '{plugin_name}' not found.")
                return
            
            # Prepare container command
            command = ["python", "-m", "bbhunt.bbhunt", "run", plugin_name]
            
            # Add target if needed
            if plugin_class.__plugin_category__ != 'utility':
                command.extend(["-t", self.current_target])
            
            # Add options if any
            if options:
                command.extend(["-o", json.dumps(options)])
            
            # Set up volumes
            volumes = {
                os.path.abspath("data"): "/app/data"
            }
            
            # Set up environment
            environment = {
                "BBHUNT_MODE": "standalone"
            }
            
            # Set up resource limits based on plugin requirements
            resources = plugin_class.__plugin_resources__
            resource_limits = {
                "memory": resources.get('memory', '500MB'),
                "cpu": resources.get('cpu', 1)
            }
            
            # Run container
            container_id = self.resource_manager.run_in_container(
                "bbhunt:latest",
                command,
                volumes,
                environment,
                resource_limits
            )
            
            click.echo(f"Running in container: {container_id[:12]}")
            
            # Simple container monitoring
            running = True
            while running:
                status = self.resource_manager.get_container_status(container_id)
                if status['status'] not in ('running', 'created'):
                    running = False
                
                click.echo(f"Status: {status['status']}")
                if status['logs']:
                    click.echo("Latest logs:")
                    click.echo(status['logs'])
                
                time.sleep(5)
            
            exit_code = status.get('exit_code')
            if exit_code == 0:
                click.echo(click.style("Container completed successfully!", fg="green"))
            else:
                click.echo(click.style(f"Container exited with code {exit_code}", fg="red"))
        
        except Exception as e:
            click.echo(click.style(f"Error running container: {str(e)}", fg="red"))
    
    def show_help(self) -> None:
        """Show help information."""
        click.echo(click.style("Available commands:", fg="green"))
        click.echo("  help                  Show this help message")
        click.echo("  plugins [category]    List available plugins")
        click.echo("  target                Select a target")
        click.echo("  target add            Add a new target")
        click.echo("  run <plugin>          Run a plugin")
        click.echo("  <plugin>              Run a plugin directly")
        click.echo("  resources             Show system resource usage")
        click.echo("  exit, quit            Exit the program")
    
    def show_resources(self) -> None:
        """Show system resource usage."""
        usage = self.resource_manager.get_resource_usage()
        
        click.echo(click.style("System Resource Usage:", fg="green"))
        
        # Memory
        click.echo(click.style("\nMemory:", fg="blue"))
        click.echo(f"  Total:     {usage['memory']['total']:.1f} MB")
        click.echo(f"  Available: {usage['memory']['available']:.1f} MB")
        click.echo(f"  Used:      {usage['memory']['used']:.1f} MB ({usage['memory']['percent']}%)")
        
        # CPU
        click.echo(click.style("\nCPU:", fg="blue"))
        click.echo(f"  Cores:     {usage['cpu']['cores']}")
        click.echo(f"  Usage:     {usage['cpu']['percent']}%")
        
        # Disk
        click.echo(click.style("\nDisk:", fg="blue"))
        click.echo(f"  Total:     {usage['disk']['total']:.1f} MB")
        click.echo(f"  Free:      {usage['disk']['free']:.1f} MB")
        click.echo(f"  Used:      {usage['disk']['used']:.1f} MB ({usage['disk']['percent']}%)")
        
        # Active processes
        if usage['processes']:
            click.echo(click.style("\nActive Processes:", fg="blue"))
            for pid, proc in usage['processes'].items():
                click.echo(f"  {proc['name']} (PID {pid}):")
                click.echo(f"    Memory:   {proc['memory_mb']:.1f} MB")
                click.echo(f"    CPU:      {proc['cpu_percent']:.1f}%")
                click.echo(f"    Runtime:  {proc['runtime']:.1f} seconds")
    
    def start_interactive(self) -> None:
        """Start interactive CLI session."""
        click.echo(click.style("Bug Bounty Framework", fg="green", bold=True))
        click.echo(click.style("Type 'help' for available commands", fg="yellow"))
        
        while True:
            # Show current target in prompt
            target_display = f"({self.current_target})" if self.current_target else "(no target)"
            command = prompt(
                f"{click.style('bbhunt', fg='green')} {click.style(target_display, fg='magenta')}> ",
                history=self.history,
                style=STYLE
            )
            
            if not command:
                continue
                
            parts = command.split()
            cmd = parts[0].lower()
            
            if cmd in ('exit', 'quit'):
                break
            elif cmd == 'help':
                self.show_help()
            elif cmd == 'plugins':
                category = parts[1] if len(parts) > 1 else None
                self.show_plugins(category)
            elif cmd == 'target':
                if len(parts) > 1 and parts[1] == 'add':
                    self.add_target()
                else:
                    self.select_target()
            elif cmd == 'run':
                if len(parts) > 1:
                    self.run_plugin(parts[1], parts[2:] if len(parts) > 2 else None)
                else:
                    click.echo("Please specify a plugin to run.")
            elif cmd == 'resources':
                self.show_resources()
            else:
                # Check if command is a plugin name
                plugin_found = False
                for category, plugins in self.plugins.items():
                    if cmd in plugins:
                        self.run_plugin(cmd, parts[1:] if len(parts) > 1 else None)
                        plugin_found = True
                        break
                
                if not plugin_found:
                    click.echo(f"Unknown command: {cmd}")

# Click CLI wrapper
@click.group(invoke_without_command=True)
@click.pass_context
def cli(ctx):
    """Bug Bounty Hunting Framework."""
    if ctx.invoked_subcommand is None:
        # Start interactive mode
        BugBountyCLI().start_interactive()

@cli.command()
@click.argument('plugin')
@click.option('--target', '-t', help='Target to run against')
@click.option('--options', '-o', help='JSON options string')
@click.option('--container', '-c', is_flag=True, help='Run in Docker container')
def run(plugin, target, options, container):
    """Run a plugin."""
    cli = BugBountyCLI()
    
    # Set target if specified
    if target:
        cli.current_target = target
        cli.config.set('current_target', target)
    
    # Parse options
    parsed_options = {}
    if options:
        try:
            parsed_options = json.loads(options)
        except json.JSONDecodeError:
            click.echo(f"Error parsing options: Invalid JSON")
            return
    
    # Run plugin
    if container and cli.resource_manager.docker_available:
        cli._run_in_container(plugin, parsed_options)
    else:
        cli.run_plugin(plugin, ['-o', options] if options else None)

@cli.command()
@click.argument('category', required=False)
def plugins(category):
    """List available plugins."""
    BugBountyCLI().show_plugins(category)

@cli.command()
def targets():
    """List or select targets."""
    BugBountyCLI().select_target()

@cli.command()
def add_target():
    """Add a new target."""
    BugBountyCLI().add_target()

@cli.command()
def resources():
    """Show system resource usage."""
    BugBountyCLI().show_resources()

if __name__ == '__main__':
    cli()
