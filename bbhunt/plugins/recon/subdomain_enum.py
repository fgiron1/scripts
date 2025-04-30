#!/usr/bin/env python3
# plugins/recon/subdomain_enum.py - Subdomain enumeration plugin

import os
import subprocess
import json
import time
from typing import Dict, Any, List, Optional
from bbhunt.core.plugin import Plugin

class SubdomainEnumPlugin(Plugin):
    """Subdomain enumeration plugin."""
    
    __plugin_name__ = "subdomain_enum"
    __plugin_description__ = "Enumerate subdomains using various tools"
    __plugin_version__ = "1.0.0"
    __plugin_category__ = "recon"
    __plugin_dependencies__ = []
    __plugin_resources__ = {
        "memory": "500MB",
        "cpu": 1,
        "disk": "50MB",
        "network": True
    }
    
    def setup(self):
        """Initialize plugin."""
        self.tools = {
            "subfinder": self._check_tool("subfinder"),
            "amass": self._check_tool("amass"),
            "assetfinder": self._check_tool("assetfinder"),
            "findomain": self._check_tool("findomain"),
            "dnsrecon": self._check_tool("dnsrecon")
        }
        
        # Track available tools
        self.available_tools = [tool for tool, available in self.tools.items() if available]
        
        if not self.available_tools:
            self.logger.warning("No subdomain enumeration tools available.")
    
    def execute(self, target: str, options: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """
        Run subdomain enumeration.
        
        Args:
            target: Target domain
            options: Plugin options
            
        Returns:
            Dict with results
        """
        options = options or {}
        passive_only = options.get('passive_only', False)
        wordlist = options.get('wordlist', '')
        user_agent = options.get('user_agent', 'bbhunt-subdomain-enum')
        
        self.logger.info(f"Running subdomain enumeration on {target}")
        self.logger.info(f"Passive only: {passive_only}")
        
        # Create output directory
        target_dir = os.path.join('data', 'targets', target, 'recon')
        os.makedirs(target_dir, exist_ok=True)
        
        # Initialize results
        results = {
            "status": "success",
            "message": "",
            "data": {
                "subdomains": [],
                "sources": {}
            }
        }
        
        # Run tools in parallel if possible
        processes = {}
        for tool in self.available_tools:
            if tool == "amass" and passive_only:
                # Add passive flag for amass
                processes[tool] = self._run_tool_async(tool, target, target_dir, passive_only=True, user_agent=user_agent)
            elif tool == "subfinder":
                processes[tool] = self._run_tool_async(tool, target, target_dir, user_agent=user_agent)
            elif tool == "assetfinder":
                processes[tool] = self._run_tool_async(tool, target, target_dir)
            elif tool == "findomain":
                processes[tool] = self._run_tool_async(tool, target, target_dir)
            elif tool == "dnsrecon":
                if not passive_only:
                    processes[tool] = self._run_tool_async(tool, target, target_dir)
        
        # Wait for all processes to finish
        for tool, process in processes.items():
            try:
                process.wait()
                self.logger.info(f"Tool {tool} finished with exit code {process.returncode}")
            except Exception as e:
                self.logger.error(f"Error waiting for {tool}: {str(e)}")
        
        # Process results from each tool
        for tool in self.available_tools:
            output_file = os.path.join(target_dir, f"{tool}_subdomains.txt")
            
            if os.path.exists(output_file):
                try:
                    with open(output_file, 'r') as f:
                        tool_subdomains = [line.strip() for line in f.readlines()]
                    
                    results["data"]["sources"][tool] = len(tool_subdomains)
                    results["data"]["subdomains"].extend(tool_subdomains)
                except Exception as e:
                    self.logger.error(f"Error processing {tool} results: {str(e)}")
        
        # Deduplicate subdomains
        results["data"]["subdomains"] = list(set(results["data"]["subdomains"]))
        
        # Save combined results
        with open(os.path.join(target_dir, 'subdomains.txt'), 'w') as f:
            for subdomain in sorted(results["data"]["subdomains"]):
                f.write(f"{subdomain}\n")
        
        # Verify live subdomains if desired
        if options.get('verify_live', True) and not passive_only:
            self._verify_live_subdomains(target_dir, results["data"]["subdomains"], user_agent)
        
        # Update result stats
        results["data"]["total"] = len(results["data"]["subdomains"])
        results["message"] = f"Found {results['data']['total']} subdomains"
        
        return results
    
    def _check_tool(self, tool: str) -> bool:
        """
        Check if a tool is available.
        
        Args:
            tool: Tool name
            
        Returns:
            True if tool is available, False otherwise
        """
        try:
            subprocess.run(
                ["which", tool], 
                stdout=subprocess.PIPE, 
                stderr=subprocess.PIPE,
                check=True
            )
            self.logger.info(f"Tool {tool} is available")
            return True
        except subprocess.CalledProcessError:
            self.logger.warning(f"Tool {tool} not found")
            return False
    
    def _run_tool_async(self, tool: str, target: str, output_dir: str, passive_only: bool = False, user_agent: str = None) -> subprocess.Popen:
        """
        Run a subdomain enumeration tool asynchronously.
        
        Args:
            tool: Tool name
            target: Target domain
            output_dir: Output directory
            passive_only: Whether to use passive mode only
            user_agent: User agent string
            
        Returns:
            Process handle
        """
        output_file = os.path.join(output_dir, f"{tool}_subdomains.txt")
        
        # Prepare environment with custom user agent if provided
        env = os.environ.copy()
        if user_agent:
            env["HTTP_USER_AGENT"] = user_agent
        
        if tool == "subfinder":
            # Subfinder command
            cmd = [
                "subfinder",
                "-d", target,
                "-o", output_file
            ]
            
            # Add custom user agent if provided
            if user_agent:
                cmd.extend(["-user-agent", user_agent])
            
            self.logger.info(f"Running subfinder for {target}")
            return subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
            
        elif tool == "amass":
            # Amass command
            cmd = [
                "amass", "enum",
                "-d", target,
                "-o", output_file
            ]
            
            # Add passive flag if requested
            if passive_only:
                cmd.append("-passive")
            
            # Add user agent if provided
            if user_agent:
                cmd.extend(["-user-agent", user_agent])
            
            self.logger.info(f"Running amass for {target}")
            return subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
            
        elif tool == "assetfinder":
            # Assetfinder command (doesn't support user agent directly)
            cmd = ["assetfinder", "--subs-only", target]
            
            self.logger.info(f"Running assetfinder for {target}")
            process = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
            
            # Save results to file in a separate thread
            def save_output():
                try:
                    output, _ = process.communicate()
                    with open(output_file, 'wb') as f:
                        f.write(output)
                except Exception as e:
                    self.logger.error(f"Error saving assetfinder output: {str(e)}")
            
            import threading
            threading.Thread(target=save_output).start()
            
            return process
            
        elif tool == "findomain":
            # Findomain command
            cmd = [
                "findomain",
                "-t", target,
                "-o", output_file
            ]
            
            self.logger.info(f"Running findomain for {target}")
            return subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
            
        elif tool == "dnsrecon":
            # DNSRecon command
            cmd = [
                "dnsrecon",
                "-d", target,
                "-t", "std,brt",
                "-j", output_file + ".json"
            ]
            
            self.logger.info(f"Running dnsrecon for {target}")
            process = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
            
            # Convert JSON output to text in a separate thread
            def convert_output():
                try:
                    process.wait()
                    if os.path.exists(output_file + ".json"):
                        with open(output_file + ".json", 'r') as f:
                            data = json.load(f)
                        
                        with open(output_file, 'w') as f:
                            for record in data:
                                if record.get("name"):
                                    f.write(f"{record['name']}\n")
                except Exception as e:
                    self.logger.error(f"Error converting dnsrecon output: {str(e)}")
            
            import threading
            threading.Thread(target=convert_output).start()
            
            return process
        
        # Fallback - return dummy process
        return subprocess.Popen(["echo", ""], stdout=subprocess.PIPE)
    
    def _verify_live_subdomains(self, target_dir: str, subdomains: List[str], user_agent: str = None) -> None:
        """
        Verify which subdomains are live.
        
        Args:
            target_dir: Target directory
            subdomains: List of subdomains to verify
            user_agent: User agent string
        """
        if not subdomains:
            return
        
        self.logger.info(f"Verifying {len(subdomains)} subdomains")
        
        # Check if httpx is available
        if self._check_tool("httpx"):
            # Create temporary file with subdomains
            temp_file = os.path.join(target_dir, "temp_subdomains.txt")
            with open(temp_file, 'w') as f:
                for subdomain in subdomains:
                    f.write(f"{subdomain}\n")
            
            # Run httpx
            output_file = os.path.join(target_dir, "live_subdomains.txt")
            cmd = [
                "httpx",
                "-l", temp_file,
                "-silent",
                "-o", output_file,
                "-threads", "50"
            ]
            
            # Add custom user agent if provided
            if user_agent:
                cmd.extend(["-H", f"User-Agent: {user_agent}"])
            
            try:
                subprocess.run(cmd, check=True)
                self.logger.info(f"Live subdomains saved to {output_file}")
            except subprocess.CalledProcessError as e:
                self.logger.error(f"Error running httpx: {str(e)}")
            
            # Clean up temporary file
            try:
                os.remove(temp_file)
            except:
                pass
    
    @classmethod
    def cli_options(cls) -> List[Dict[str, Any]]:
        """Define CLI options for this plugin."""
        return [
            {
                "name": "--passive",
                "help": "Run passive techniques only",
                "is_flag": True
            },
            {
                "name": "--wordlist",
                "help": "Wordlist for brute force (if supported)",
                "type": str
            },
            {
                "name": "--user-agent",
                "help": "Custom User-Agent string",
                "type": str,
                "default": "bbhunt-subdomain-enum"
            },
            {
                "name": "--no-verify",
                "help": "Skip verification of live subdomains",
                "is_flag": True
            }
        ]
    
    @classmethod
    def interactive_options(cls) -> List[Dict[str, Any]]:
        """Define interactive prompts for this plugin."""
        return [
            {
                "type": "confirm",
                "name": "passive_only",
                "message": "Run passive techniques only?",
                "default": False
            },
            {
                "type": "input",
                "name": "wordlist",
                "message": "Custom wordlist path (optional):",
                "default": ""
            },
            {
                "type": "input",
                "name": "user_agent",
                "message": "Custom User-Agent string:",
                "default": "bbhunt-subdomain-enum"
            },
            {
                "type": "confirm",
                "name": "verify_live",
                "message": "Verify live subdomains?",
                "default": True
            }
        ]
