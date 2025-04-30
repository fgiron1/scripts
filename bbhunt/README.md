# Bug Bounty Framework (BBHunt)

A modular, extensible framework for bug bounty hunting with containerization support.

## Features

- **Plugin-Based Architecture**: Easily add new tools and techniques
- **Resource-Aware**: Built-in resource monitoring prevents crashes on limited hardware
- **Containerization**: Docker support for distributed execution and parallel testing
- **Interactive CLI**: Rich command-line interface with intelligent suggestions
- **HackerOne Optimized**: Special features for HackerOne bug bounty programs

## Quick Start

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/bbhunt.git
cd bbhunt

# Install dependencies
pip install -r requirements.txt

# Create necessary directories
mkdir -p data config wordlists
```

### Basic Usage

```bash
# Add a target
python -m bbhunt.bbhunt target add example.com

# Run reconnaissance
python -m bbhunt.bbhunt run subdomain_enum -t example.com

# Launch interactive mode
python -m bbhunt.bbhunt
```

### Docker Usage

```bash
# Build the image
docker build -t bbhunt:latest -f docker/Dockerfile .

# Run a container
docker run -it --rm -v $(pwd)/data:/app/data bbhunt:latest run subdomain_enum -t example.com

# Or start the full distributed environment
docker-compose up -d
```

## Available Modules

The framework includes several core modules:

| Category | Module | Description |
|----------|--------|-------------|
| Recon | subdomain_enum | Enumerate subdomains using various tools |
| Recon | content_discovery | Find hidden files and directories |
| Scan | web_scan | Scan web applications for vulnerabilities |
| Exploit | xss_verify | Verify XSS vulnerabilities |
| Exploit | ssrf_verify | Verify SSRF vulnerabilities |

## Example Workflow for Audible Bug Bounty

```bash
# Add target
bbhunt target add audible.com

# Run subdomain enumeration with program-specific requirements
bbhunt run subdomain_enum -o '{
  "user_agent": "audibleresearcher_yourusername",
  "rate_limit": 5
}'

# Run web scanning
bbhunt run web_scan -o '{
  "user_agent": "audibleresearcher_yourusername",
  "mode": "standard",
  "exclude": "DOS,CSRF,Missing Cookie Flags"
}'

# Test for XSS
bbhunt run xss_verify -o '{
  "url": "https://www.audible.com/search?keywords=INJECT",
  "user_agent": "audibleresearcher_yourusername"
}'
```

## Project Structure

```
bbhunt/
├── core/               # Core framework components
├── plugins/            # Plugin modules
│   ├── recon/          # Reconnaissance plugins
│   ├── scan/           # Vulnerability scanning plugins
│   ├── exploit/        # Exploitation plugins
│   └── report/         # Reporting plugins
├── data/               # Data storage
├── config/             # Configuration files
├── docker/             # Docker configuration
├── requirements.txt    # Python dependencies
└── bbhunt.py           # Main executable
```

## Creating Custom Plugins

Plugins inherit from the `Plugin` base class and implement the required methods:

```python
from bbhunt.core.plugin import Plugin

class MyCustomPlugin(Plugin):
    """My custom plugin."""
    
    __plugin_name__ = "my_custom_plugin"
    __plugin_description__ = "Description of what my plugin does"
    __plugin_version__ = "1.0.0"
    __plugin_category__ = "custom"
    
    def execute(self, target, options=None):
        """Execute the plugin."""
        # Implementation here
        return {"status": "success", "data": {}}
```

## Resource Management

The framework includes built-in resource management to prevent crashes on limited hardware:

```bash
# Check available resources
bbhunt resources

# Run a resource-intensive plugin in a container
bbhunt run web_scan -t example.com --container
```

## Documentation

For detailed documentation, see the [Documentation](Documentation.md) file.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
