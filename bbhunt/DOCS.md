# Bug Bounty Framework Documentation

## Overview

The Bug Bounty Framework (BBHunt) is a modular, extensible platform for security researchers to streamline bug bounty hunting activities. Its plugin-based architecture allows for easy addition of new tools and techniques while the containerization support enables efficient resource management and parallel testing across multiple targets.

## Table of Contents

1. [Installation](#installation)
2. [Architecture](#architecture)
3. [Core Components](#core-components)
4. [Plugins](#plugins)
5. [Command Line Interface](#command-line-interface)
6. [Containerization](#containerization)
7. [Workflow Examples](#workflow-examples)
8. [Customization](#customization)
9. [Audible Bug Bounty Example](#audible-bug-bounty-example)

## Installation

### Basic Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/bbhunt.git
cd bbhunt

# Install dependencies
pip install -r requirements.txt

# Create necessary directories
mkdir -p data config wordlists

# Install Go-based tools (optional)
go install -v github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest
go install -v github.com/projectdiscovery/httpx/cmd/httpx@latest
go install -v github.com/projectdiscovery/nuclei/v2/cmd/nuclei@latest
go install -v github.com/ffuf/ffuf@latest
```

### Docker Installation

```bash
# Build the Docker image
docker build -t bbhunt:latest -f docker/Dockerfile .

# Run a single command
docker run -it --rm -v $(pwd)/data:/app/data bbhunt:latest run subdomain_enum -t example.com

# Or start the distributed environment
docker-compose up -d
```

## Architecture

The framework is built with modularity and extensibility as its core principles:

1. **Plugin System**: All functionality is implemented through plugins, allowing easy extension.
2. **Resource Management**: Built-in resource monitoring ensures stability on limited hardware.
3. **Containerization**: Docker support enables distributed execution and parallel testing.
4. **CLI + API**: Dual interfaces for both interactive use and automation.

### Directory Structure

```
bbhunt/
├── core/               # Core framework components
│   ├── cli.py          # Command line interface
│   ├── plugin.py       # Plugin system
│   ├── resources.py    # Resource management
│   └── config.py       # Configuration management
│
├── plugins/            # Plugin directories by category
│   ├── recon/          # Reconnaissance plugins
│   ├── scan/           # Vulnerability scanning plugins
│   ├── exploit/        # Exploitation plugins
│   └── report/         # Reporting plugins
│
├── data/               # Data storage
│   └── targets/        # Target data organized by domain
│
├── config/             # Configuration files
│
├── docker/             # Docker configuration
│   └── Dockerfile      # Base container definition
│
├── requirements.txt    # Python dependencies
└── bbhunt.py           # Main executable
```

## Core Components

### Plugin System (`core/plugin.py`)

The foundation of the framework is its plugin system. All functionality is implemented as plugins that inherit from the `Plugin` base class:

```python
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
    
    def setup(self):
        """Set up the plugin."""
        pass
    
    def execute(self, target, options=None):
        """Execute the plugin."""
        raise NotImplementedError("Plugin must implement execute method")
    
    def cleanup(self):
        """Clean up after plugin execution."""
        pass
```

### Resource Management (`core/resources.py`)

The resource management system monitors and controls system resource usage to prevent crashes or excessive load:

```python
# Check if system has enough resources
def check_resources(self, requirements):
    avail_memory = psutil.virtual_memory().available
    
    if requirements.memory_mb * 1024 * 1024 > avail_memory:
        return False, "Not enough memory"
    
    return True, "Sufficient resources available"
```

### Configuration Management (`core/config.py`)

The configuration system manages persistent settings and plugin configurations:

```python
def get(self, key, default=None):
    """Get a configuration value."""
    return self.config.get(key, default)
    
def set(self, key, value):
    """Set a configuration value."""
    self.config[key] = value
    self._save_config()
```

### Command Line Interface (`core/cli.py`)

The CLI provides both interactive and command-based interfaces to the framework:

```python
# Interactive mode
bbhunt

# Direct plugin execution
bbhunt run subdomain_enum -t example.com
```

## Plugins

Plugins are organized by category:

### Reconnaissance Plugins

- **subdomain_enum**: Enumerate subdomains using various tools and techniques
- **content_discovery**: Find hidden files and directories on web servers

### Vulnerability Scanning Plugins

- **web_scan**: Scan web applications for vulnerabilities using tools like Nuclei

### Exploitation Plugins

- **xss_verify**: Verify and exploit XSS vulnerabilities

### Reporting Plugins

- *Coming soon*

## Command Line Interface

The framework provides both interactive and command-based interfaces:

### Interactive Mode

```bash
$ bbhunt
bbhunt (no target)> target add example.com
Added target: example.com
bbhunt (example.com)> plugins
Available plugins:

RECON:
  subdomain_enum - Enumerate subdomains using various tools
  content_discovery - Discover hidden files and directories

SCAN:
  web_scan - Scan web applications for vulnerabilities

EXPLOIT:
  xss_verify - Verify XSS vulnerabilities

bbhunt (example.com)> run subdomain_enum
Running subdomain_enum on example.com...
```

### Command Mode

```bash
# Run a plugin directly
bbhunt run subdomain_enum -t example.com

# Pass options as JSON
bbhunt run web_scan -t example.com -o '{"mode": "thorough", "user_agent": "bbhunt-scan"}'

# List available plugins
bbhunt plugins

# Add a target
bbhunt target add example.com
```

## Containerization

The framework supports containerization for efficient resource usage and parallel testing:

### Single Container

```bash
# Run a single plugin in a container
docker run -it --rm \
  -v $(pwd)/data:/app/data \
  bbhunt:latest \
  run subdomain_enum -t example.com
```

### Distributed Mode

Using Docker Compose, you can run multiple components in parallel:

```bash
# Start the distributed environment
docker-compose up -d

# Scale workers as needed
docker-compose up -d --scale recon-worker=4

# Submit tasks via API
curl -X POST http://localhost:8080/tasks \
  -H "Content-Type: application/json" \
  -d '{"plugin": "subdomain_enum", "target": "example.com"}'
```

## Workflow Examples

### Basic Bug Hunting Workflow

```bash
# Add a target
bbhunt target add example.com

# Run reconnaissance
bbhunt run subdomain_enum
bbhunt run content_discovery

# Scan for vulnerabilities
bbhunt run web_scan -o '{"mode": "thorough"}'

# Verify potential vulnerabilities
bbhunt run xss_verify -o '{"url": "https://example.com/search?q=INJECT"}'
```

### Containerized Workflow

```bash
# Start the environment
docker-compose up -d

# Submit tasks via CLI
docker-compose exec bbhunt-core python -m bbhunt.bbhunt run subdomain_enum -t example.com

# Or via API
curl -X POST http://localhost:8080/tasks \
  -H "Content-Type: application/json" \
  -d '{"plugin": "subdomain_enum", "target": "example.com"}'

# Check task status
curl http://localhost:8080/tasks
```

## Customization

### Creating a Custom Plugin

1. Create a new Python file in the appropriate category directory:

```python
# plugins/custom/my_plugin.py
from bbhunt.core.plugin import Plugin

class MyCustomPlugin(Plugin):
    """My custom plugin."""
    
    __plugin_name__ = "my_custom_plugin"
    __plugin_description__ = "Description of what my plugin does"
    __plugin_version__ = "1.0.0"
    __plugin_category__ = "custom"
    __plugin_dependencies__ = []
    __plugin_resources__ = {
        "memory": "200MB",
        "cpu": 1,
        "disk": "50MB",
        "network": True
    }
    
    def setup(self):
        """Initialize plugin."""
        pass
    
    def execute(self, target, options=None):
        """Execute the plugin."""
        options = options or {}
        
        # Plugin implementation
        
        return {
            "status": "success",
            "message": "Plugin completed successfully",
            "data": {}
        }
    
    @classmethod
    def interactive_options(cls):
        """Define interactive prompts for this plugin."""
        return [
            {
                "type": "input",
                "name": "param1",
                "message": "Enter parameter 1:",
                "default": "default_value"
            }
        ]
```

2. The plugin will be automatically discovered and available in the framework.

### Extending Existing Plugins

You can extend or override existing plugins by creating a new plugin with the same name in a different location and adding it to the Python path.

## Audible Bug Bounty Example

The following example demonstrates how to use the framework for the Audible bug bounty program:

### 1. Initial Setup

```bash
# Add Audible as a target
bbhunt target add audible.com

# Create a session
bbhunt session start
```

### 2. Subdomain Enumeration (Respecting Program Rules)

```bash
# Run subdomain enumeration with appropriate User-Agent
bbhunt run subdomain_enum -o '{
  "user_agent": "audibleresearcher_yourusername",
  "rate_limit": 5
}'
```

This will discover subdomains under `*.audible.*` as specified in the program scope.

### 3. Content Discovery

```bash
# Discover content with rate limiting
bbhunt run content_discovery -o '{
  "user_agent": "audibleresearcher_yourusername",
  "rate_limit": 5,
  "threads": 5
}'
```

### 4. Vulnerability Scanning

```bash
# Run web vulnerability scanning
bbhunt run web_scan -o '{
  "user_agent": "audibleresearcher_yourusername",
  "rate_limit": 5,
  "mode": "standard",
  "exclude": "DOS,CSRF,Missing Cookie Flags"
}'
```

The exclusions ensure we respect the out-of-scope vulnerabilities specified in the program.

### 5. Testing for XSS (In Scope for Audible)

```bash
# Verify potential XSS issues
bbhunt run xss_verify -o '{
  "url": "https://www.audible.com/search?keywords=INJECT",
  "user_agent": "audibleresearcher_yourusername"
}'
```

### 6. Mobile App Analysis (In Scope for Audible)

```bash
# Analyze Android app (requires downloading APK first)
bbhunt run mobile_static_analysis -o '{
  "input_file": "audible.apk",
  "check_exported_components": true
}'
```

### 7. Report Generation for HackerOne

```bash
# Generate HackerOne report
bbhunt report h1 -i "high_xss_20230501123045" -f "markdown"
```

Throughout this workflow, we respect the program's requirements:
- Using the required User-Agent string
- Respecting rate limits (max 5 requests per second)
- Excluding out-of-scope vulnerability types
- Testing only in-scope assets

This systematic approach allows for efficient bug hunting while maintaining compliance with the program rules.
