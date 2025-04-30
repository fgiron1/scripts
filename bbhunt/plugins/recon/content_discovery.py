#!/usr/bin/env python3
# plugins/recon/content_discovery.py - Content discovery plugin

import os
import subprocess
import json
import time
import re
from typing import Dict, Any, List, Optional
from bbhunt.core.plugin import Plugin

class ContentDiscoveryPlugin(Plugin):
    """Content discovery plugin for finding hidden files and directories."""
    
    __plugin_name__ = "content_discovery"
    __plugin_description__ = "Discover hidden files and directories"
    __plugin_version__ = "1.0.0"
    __plugin_category__ = "recon"
    __plugin_dependencies__ = []
    __plugin_resources__ = {
        "memory": "800MB",
        "cpu": 2,
        "disk": "100MB",
        "network": True
    }
    
    def setup(self):
        """Initialize plugin."""
        self.tools = {
            "ffuf": self._check_tool("ffuf"),
            "gobuster": self._check_tool("gobuster"),
            "dirsearch": self._check_tool("dirsearch"),
            "feroxbuster": self._check_tool("feroxbuster")
        }
        
        # Track available tools
        self.available_tools = [tool for tool, available in self.tools.items() if available]
        
        if not self.available_tools:
            self.logger.warning("No content discovery tools available.")
    
    def execute(self, target: str, options: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """
        Run content discovery.
        
        Args:
            target: Target domain
            options: Plugin options
            
        Returns:
            Dict with results
        """
        options = options or {}
        
        # Parse options
        wordlist = options.get('wordlist', '')
        threads = min(int(options.get('threads', 10)), 50)  # Limit threads to 50
        user_agent = options.get('user_agent', 'bbhunt-content-discovery')
        input_file = options.get('input_file', '')
        extensions = options.get('extensions', 'php,asp,aspx,jsp,html,js,txt')
        tool = options.get('tool', '')  # Preferred tool, if any
        
        # Set default wordlist if not specified
        if not wordlist:
            wordlist_paths = [
                os.path.join('wordlists', 'SecLists', 'Discovery', 'Web-Content', 'directory-list-2.3-medium.txt'),
                os.path.join('wordlists', 'directory-list-2.3-medium.txt'),
                os.path.join('wordlists', 'content-discovery.txt')
            ]
            
            for path in wordlist_paths:
                if os.path.exists(path):
                    wordlist = path
                    break
            
            if not wordlist:
                return {
                    "status": "error",
                    "message": "No wordlist provided and no default wordlist found",
                    "data": {}
                }
        
        self.logger.info(f"Running content discovery with wordlist: {wordlist}")
        
        # Process targets
        targets = []
        
        if input_file:
            # Read targets from file
            if os.path.exists(input_file):
                with open(input_file, 'r') as f:
                    targets = [line.strip() for line in f.readlines()]
            else:
                return {
                    "status": "error",
                    "message": f"Input file not found: {input_file}",
                    "data": {}
                }
        else:
            # Use single target
            targets = [target]
        
        # Ensure all targets have http:// or https:// prefix
        processed_targets = []
        for t in targets:
            if not t.startswith(('http://', 'https://')):
                # Try https first, then http if that fails
                processed_targets.append(f"https://{t}")
                # We'll check connectivity later
            else:
                processed_targets.append(t)
        
        targets = processed_targets
        
        # Initialize results
        results = {
            "status": "success",
            "message": "",
            "data": {
                "findings": {},
                "stats": {
                    "total_targets": len(targets),
                    "success": 0,
                    "failed": 0,
                    "total_findings": 0
                }
            }
        }
        
        # Determine which tool to use
        chosen_tool = None
        
        if tool and tool in self.available_tools:
            chosen_tool = tool
        elif self.available_tools:
            # Prefer feroxbuster > ffuf > gobuster > dirsearch
            for preferred in ['feroxbuster', 'ffuf', 'gobuster', 'dirsearch']:
                if preferred in self.available_tools:
                    chosen_tool = preferred
                    break
        
        if not chosen_tool:
            return {
                "status": "error",
                "message": "No content discovery tools available",
                "data": {}
            }
        
        self.logger.info(f"Using {chosen_tool} for content discovery")
        
        # Process each target
        for target_url in targets:
            target_domain = target_url.replace('https://', '').replace('http://', '').split('/')[0]
            
            # Create output directory
            target_dir = os.path.join('data', 'targets', target, 'recon', 'content')
            os.makedirs(target_dir, exist_ok=True)
            
            # Run content discovery
            try:
                if chosen_tool == 'ffuf':
                    findings = self._run_ffuf(target_url, wordlist, target_dir, threads, extensions, user_agent)
                elif chosen_tool == 'gobuster':
                    findings = self._run_gobuster(target_url, wordlist, target_dir, threads, extensions, user_agent)
                elif chosen_tool == 'dirsearch':
                    findings = self._run_dirsearch(target_url, wordlist, target_dir, threads, extensions, user_agent)
                elif chosen_tool == 'feroxbuster':
                    findings = self._run_feroxbuster(target_url, wordlist, target_dir, threads, extensions, user_agent)
                else:
                    raise ValueError(f"Unknown tool: {chosen_tool}")
                
                # Store findings
                results["data"]["findings"][target_url] = findings
                results["data"]["stats"]["success"] += 1
                results["data"]["stats"]["total_findings"] += len(findings)
                
                self.logger.info(f"Found {len(findings)} resources for {target_url}")
            except Exception as e:
                self.logger.error(f"Error scanning {target_url}: {str(e)}")
                results["data"]["stats"]["failed"] += 1
        
        # Save combined results to a summary file
        summary_file = os.path.join('data', 'targets', target, 'recon', 'content_discovery_summary.json')
        
        try:
            with open(summary_file, 'w') as f:
                json.dump(results["data"], f, indent=2)
            
            self.logger.info(f"Content discovery summary saved to {summary_file}")
        except Exception as e:
            self.logger.error(f"Error saving summary: {str(e)}")
        
        # Create a list of all discovered endpoints
        all_endpoints = []
        for target_url, findings in results["data"]["findings"].items():
            for finding in findings:
                all_endpoints.append(finding["url"])
        
        # Save all endpoints to a file
        endpoints_file = os.path.join('data', 'targets', target, 'recon', 'live_endpoints.txt')
        
        try:
            with open(endpoints_file, 'w') as f:
                for endpoint in sorted(all_endpoints):
                    f.write(f"{endpoint}\n")
            
            self.logger.info(f"All endpoints saved to {endpoints_file}")
        except Exception as e:
            self.logger.error(f"Error saving endpoints: {str(e)}")
        
        # Update result message
        results["message"] = f"Found {results['data']['stats']['total_findings']} resources across {results['data']['stats']['success']} targets"
        
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
    
    def _run_ffuf(self, target_url: str, wordlist: str, output_dir: str, threads: int, extensions: str, user_agent: str) -> List[Dict[str, Any]]:
        """
        Run ffuf for content discovery.
        
        Args:
            target_url: Target URL
            wordlist: Path to wordlist
            output_dir: Output directory
            threads: Number of threads
            extensions: File extensions to check
            user_agent: User agent string
            
        Returns:
            List of findings
        """
        self.logger.info(f"Running ffuf for {target_url}")
        
        # Output file
        output_file = os.path.join(output_dir, f"ffuf_{target_url.replace('://', '_').replace('/', '_')}.json")
        
        # Build ffuf command
        cmd = [
            "ffuf",
            "-u", f"{target_url}/FUZZ",
            "-w", wordlist,
            "-t", str(threads),
            "-mc", "200,201,202,203,204,301,302,307,401,403,405",
            "-o", output_file,
            "-of", "json"
        ]
        
        # Add extensions if provided
        if extensions:
            cmd.extend(["-e", extensions])
        
        # Add user agent if provided
        if user_agent:
            cmd.extend(["-H", f"User-Agent: {user_agent}"])
        
        # Run ffuf
        try:
            subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            self.logger.error(f"Error running ffuf: {e.stderr.decode()}")
            return []
        
        # Parse results
        if not os.path.exists(output_file):
            self.logger.error(f"Output file not found: {output_file}")
            return []
        
        try:
            with open(output_file, 'r') as f:
                data = json.load(f)
            
            findings = []
            for result in data.get("results", []):
                finding = {
                    "url": result.get("url", ""),
                    "status": result.get("status", 0),
                    "size": result.get("length", 0),
                    "words": result.get("words", 0),
                    "lines": result.get("lines", 0)
                }
                findings.append(finding)
            
            return findings
        except Exception as e:
            self.logger.error(f"Error parsing ffuf results: {str(e)}")
            return []
    
    def _run_gobuster(self, target_url: str, wordlist: str, output_dir: str, threads: int, extensions: str, user_agent: str) -> List[Dict[str, Any]]:
        """
        Run gobuster for content discovery.
        
        Args:
            target_url: Target URL
            wordlist: Path to wordlist
            output_dir: Output directory
            threads: Number of threads
            extensions: File extensions to check
            user_agent: User agent string
            
        Returns:
            List of findings
        """
        self.logger.info(f"Running gobuster for {target_url}")
        
        # Output file
        output_file = os.path.join(output_dir, f"gobuster_{target_url.replace('://', '_').replace('/', '_')}.txt")
        
        # Build gobuster command
        cmd = [
            "gobuster", "dir",
            "-u", target_url,
            "-w", wordlist,
            "-t", str(threads),
            "-o", output_file
        ]
        
        # Add extensions if provided
        if extensions:
            cmd.extend(["-x", extensions])
        
        # Add user agent if provided
        if user_agent:
            cmd.extend(["-a", user_agent])
        
        # Run gobuster
        try:
            subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            self.logger.error(f"Error running gobuster: {e.stderr.decode()}")
            return []
        
        # Parse results
        if not os.path.exists(output_file):
            self.logger.error(f"Output file not found: {output_file}")
            return []
        
        try:
            findings = []
            with open(output_file, 'r') as f:
                for line in f:
                    # Gobuster output format: /path (Status: 200) [Size: 1234]
                    match = re.search(r'(\/[^\s]*) \(Status: (\d+)\)(?: \[Size: (\d+)\])?', line)
                    if match:
                        path, status, size = match.groups()
                        size = int(size) if size else 0
                        
                        finding = {
                            "url": f"{target_url}{path}",
                            "status": int(status),
                            "size": size,
                            "words": 0,  # Not provided by gobuster
                            "lines": 0   # Not provided by gobuster
                        }
                        findings.append(finding)
            
            return findings
        except Exception as e:
            self.logger.error(f"Error parsing gobuster results: {str(e)}")
            return []
    
    def _run_dirsearch(self, target_url: str, wordlist: str, output_dir: str, threads: int, extensions: str, user_agent: str) -> List[Dict[str, Any]]:
        """
        Run dirsearch for content discovery.
        
        Args:
            target_url: Target URL
            wordlist: Path to wordlist
            output_dir: Output directory
            threads: Number of threads
            extensions: File extensions to check
            user_agent: User agent string
            
        Returns:
            List of findings
        """
        self.logger.info(f"Running dirsearch for {target_url}")
        
        # Output file
        output_file = os.path.join(output_dir, f"dirsearch_{target_url.replace('://', '_').replace('/', '_')}.json")
        
        # Build dirsearch command
        cmd = [
            "dirsearch",
            "-u", target_url,
            "-w", wordlist,
            "-t", str(threads),
            "-o", output_file,
            "--format=json"
        ]
        
        # Add extensions if provided
        if extensions:
            cmd.extend(["-e", extensions])
        
        # Add user agent if provided
        if user_agent:
            cmd.extend(["--user-agent", user_agent])
        
        # Run dirsearch
        try:
            subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            self.logger.error(f"Error running dirsearch: {e.stderr.decode()}")
            return []
        
        # Parse results
        if not os.path.exists(output_file):
            self.logger.error(f"Output file not found: {output_file}")
            return []
        
        try:
            with open(output_file, 'r') as f:
                data = json.load(f)
            
            findings = []
            for url, results in data.items():
                for path, result in results.items():
                    finding = {
                        "url": f"{url}{path}",
                        "status": result.get("status", 0),
                        "size": result.get("content-length", 0),
                        "words": 0,  # Not provided by dirsearch
                        "lines": 0   # Not provided by dirsearch
                    }
                    findings.append(finding)
            
            return findings
        except Exception as e:
            self.logger.error(f"Error parsing dirsearch results: {str(e)}")
            return []
    
    def _run_feroxbuster(self, target_url: str, wordlist: str, output_dir: str, threads: int, extensions: str, user_agent: str) -> List[Dict[str, Any]]:
        """
        Run feroxbuster for content discovery.
        
        Args:
            target_url: Target URL
            wordlist: Path to wordlist
            output_dir: Output directory
            threads: Number of threads
            extensions: File extensions to check
            user_agent: User agent string
            
        Returns:
            List of findings
        """
        self.logger.info(f"Running feroxbuster for {target_url}")
        
        # Output file
        output_file = os.path.join(output_dir, f"feroxbuster_{target_url.replace('://', '_').replace('/', '_')}.json")
        
        # Build feroxbuster command
        cmd = [
            "feroxbuster",
            "--url", target_url,
            "--wordlist", wordlist,
            "--threads", str(threads),
            "--output", output_file,
            "--json"
        ]
        
        # Add extensions if provided
        if extensions:
            for ext in extensions.split(','):
                cmd.extend(["--extensions", ext])
        
        # Add user agent if provided
        if user_agent:
            cmd.extend(["--user-agent", user_agent])
        
        # Run feroxbuster
        try:
            subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            self.logger.error(f"Error running feroxbuster: {e.stderr.decode()}")
            return []
        
        # Parse results
        if not os.path.exists(output_file):
            self.logger.error(f"Output file not found: {output_file}")
            return []
        
        try:
            findings = []
            with open(output_file, 'r') as f:
                for line in f:
                    try:
                        result = json.loads(line)
                        finding = {
                            "url": result.get("url", ""),
                            "status": result.get("status", 0),
                            "size": result.get("content_length", 0),
                            "words": result.get("word_count", 0),
                            "lines": result.get("line_count", 0)
                        }
                        findings.append(finding)
                    except json.JSONDecodeError:
                        continue
            
            return findings
        except Exception as e:
            self.logger.error(f"Error parsing feroxbuster results: {str(e)}")
            return []
    
    @classmethod
    def cli_options(cls) -> List[Dict[str, Any]]:
        """Define CLI options for this plugin."""
        return [
            {
                "name": "--wordlist",
                "help": "Path to wordlist",
                "type": str
            },
            {
                "name": "--threads",
                "help": "Number of threads (max 50)",
                "type": int,
                "default": 10
            },
            {
                "name": "--extensions",
                "help": "File extensions to check (comma-separated)",
                "type": str,
                "default": "php,asp,aspx,jsp,html,js,txt"
            },
            {
                "name": "--input-file",
                "help": "File containing target URLs",
                "type": str
            },
            {
                "name": "--user-agent",
                "help": "Custom User-Agent string",
                "type": str,
                "default": "bbhunt-content-discovery"
            },
            {
                "name": "--tool",
                "help": "Preferred tool (ffuf, gobuster, dirsearch, feroxbuster)",
                "type": str
            }
        ]
    
    @classmethod
    def interactive_options(cls) -> List[Dict[str, Any]]:
        """Define interactive prompts for this plugin."""
        return [
            {
                "type": "input",
                "name": "wordlist",
                "message": "Path to wordlist (leave empty for default):",
                "default": ""
            },
            {
                "type": "input",
                "name": "threads",
                "message": "Number of threads (max 50):",
                "default": "10"
            },
            {
                "type": "input",
                "name": "extensions",
                "message": "File extensions to check (comma-separated):",
                "default": "php,asp,aspx,jsp,html,js,txt"
            },
            {
                "type": "input",
                "name": "input_file",
                "message": "File containing target URLs (leave empty to use current target):",
                "default": ""
            },
            {
                "type": "input",
                "name": "user_agent",
                "message": "Custom User-Agent string:",
                "default": "bbhunt-content-discovery"
            },
            {
                "type": "input",
                "name": "tool",
                "message": "Preferred tool (leave empty for auto-select):",
                "default": ""
            }
        ]
