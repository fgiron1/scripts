#!/usr/bin/env python3
# plugins/scan/web_scan.py - Web application vulnerability scanner

import os
import subprocess
import json
import time
import re
import requests
from urllib.parse import urlparse
from typing import Dict, Any, List, Optional
from bbhunt.core.plugin import Plugin

class WebScanPlugin(Plugin):
    """Web application vulnerability scanner."""
    
    __plugin_name__ = "web_scan"
    __plugin_description__ = "Scan web applications for vulnerabilities"
    __plugin_version__ = "1.0.0"
    __plugin_category__ = "scan"
    __plugin_dependencies__ = []
    __plugin_resources__ = {
        "memory": "1GB",
        "cpu": 2,
        "disk": "200MB",
        "network": True
    }
    
    def setup(self):
        """Initialize plugin."""
        self.tools = {
            "nuclei": self._check_tool("nuclei"),
            "nikto": self._check_tool("nikto"),
            "zap-cli": self._check_tool("zap-cli"),
            "whatweb": self._check_tool("whatweb"),
            "sqlmap": self._check_tool("sqlmap")
        }
        
        # Track available tools
        self.available_tools = [tool for tool, available in self.tools.items() if available]
        
        if not self.available_tools:
            self.logger.warning("No web scanning tools available.")
    
    def execute(self, target: str, options: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """
        Run web application vulnerability scan.
        
        Args:
            target: Target domain or URL
            options: Plugin options
            
        Returns:
            Dict with results
        """
        options = options or {}
        
        # Parse options
        mode = options.get('mode', 'standard')  # basic, standard, thorough
        input_file = options.get('input_file', '')
        user_agent = options.get('user_agent', 'bbhunt-web-scan')
        rate_limit = int(options.get('rate_limit', 10))
        exclude = options.get('exclude', '').split(',') if options.get('exclude') else []
        
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
            # Use single target - ensure it has http:// or https:// prefix
            if not target.startswith(('http://', 'https://')):
                targets = [f"https://{target}"]
            else:
                targets = [target]
        
        # Create output directory
        target_domain = urlparse(targets[0]).netloc.split(':')[0] if targets else target
        scan_dir = os.path.join('data', 'targets', target, 'scan')
        os.makedirs(scan_dir, exist_ok=True)
        
        # Initialize results
        results = {
            "status": "success",
            "message": "",
            "data": {
                "vulnerabilities": [],
                "scan_info": {
                    "mode": mode,
                    "targets_scanned": len(targets),
                    "tools_used": []
                }
            }
        }
        
        # Run various scans based on available tools and mode
        if self.tools["nuclei"]:
            vulnerabilities = self._run_nuclei(targets, scan_dir, mode, user_agent, rate_limit, exclude)
            results["data"]["vulnerabilities"].extend(vulnerabilities)
            results["data"]["scan_info"]["tools_used"].append("nuclei")
        
        if self.tools["nikto"] and mode in ('thorough', 'all'):
            vulnerabilities = self._run_nikto(targets, scan_dir, user_agent)
            results["data"]["vulnerabilities"].extend(vulnerabilities)
            results["data"]["scan_info"]["tools_used"].append("nikto")
        
        if self.tools["whatweb"]:
            tech_info = self._run_whatweb(targets, scan_dir, user_agent)
            results["data"]["tech_info"] = tech_info
            results["data"]["scan_info"]["tools_used"].append("whatweb")
        
        # Additional specialized scans for thorough mode
        if mode == 'thorough':
            if self.tools["sqlmap"] and not 'sqli' in exclude:
                vulnerabilities = self._run_sqlmap_discovery(targets, scan_dir, user_agent, rate_limit)
                results["data"]["vulnerabilities"].extend(vulnerabilities)
                results["data"]["scan_info"]["tools_used"].append("sqlmap")
        
        # Deduplicate vulnerabilities
        unique_vulns = []
        seen = set()
        
        for vuln in results["data"]["vulnerabilities"]:
            # Create a key for deduplication
            key = f"{vuln.get('type')}:{vuln.get('url')}:{vuln.get('name')}"
            
            if key not in seen:
                seen.add(key)
                unique_vulns.append(vuln)
        
        results["data"]["vulnerabilities"] = unique_vulns
        results["data"]["total_vulnerabilities"] = len(unique_vulns)
        
        # Calculate severity counts
        severity_counts = {
            "critical": 0,
            "high": 0,
            "medium": 0,
            "low": 0,
            "info": 0
        }
        
        for vuln in unique_vulns:
            severity = vuln.get('severity', '').lower()
            if severity in severity_counts:
                severity_counts[severity] += 1
        
        results["data"]["severity_counts"] = severity_counts
        
        # Save results to file
        output_file = os.path.join(scan_dir, 'web_scan_results.json')
        
        try:
            with open(output_file, 'w') as f:
                json.dump(results["data"], f, indent=2)
        except Exception as e:
            self.logger.error(f"Error saving results to {output_file}: {str(e)}")
        
        # Update result message
        results["message"] = f"Found {len(unique_vulns)} vulnerabilities ({severity_counts['critical']} critical, {severity_counts['high']} high)"
        
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
    
    def _run_nuclei(self, targets: List[str], output_dir: str, mode: str, user_agent: str, rate_limit: int, exclude: List[str]) -> List[Dict[str, Any]]:
        """
        Run nuclei for vulnerability scanning.
        
        Args:
            targets: List of target URLs
            output_dir: Output directory
            mode: Scan mode (basic, standard, thorough)
            user_agent: User agent string
            rate_limit: Rate limit
            exclude: List of vulnerability types to exclude
            
        Returns:
            List of vulnerabilities
        """
        self.logger.info(f"Running nuclei with mode: {mode}")
        
        # Create targets file
        targets_file = os.path.join(output_dir, 'nuclei_targets.txt')
        with open(targets_file, 'w') as f:
            for target in targets:
                f.write(f"{target}\n")
        
        # Define templates based on mode
        templates = []
        
        if mode == 'basic':
            templates = ["cves"]
        elif mode == 'standard':
            templates = ["cves", "vulnerabilities", "misconfiguration"]
        else:  # thorough
            templates = ["cves", "vulnerabilities", "misconfiguration", "exposures", "technologies"]
        
        # Add appropriate flags for each template
        template_args = []
        for template in templates:
            template_args.extend(["-t", template])
        
        # Output file
        output_file = os.path.join(output_dir, 'nuclei_results.json')
        
        # Build nuclei command
        cmd = [
            "nuclei",
            "-l", targets_file,
            "-json",
            "-o", output_file,
            "-rate-limit", str(rate_limit)
        ]
        
        # Add user agent if provided
        if user_agent:
            cmd.extend(["-H", f"User-Agent: {user_agent}"])
        
        # Add template arguments
        cmd.extend(template_args)
        
        # Add exclude patterns
        for pattern in exclude:
            cmd.extend(["-exclude", pattern])
        
        # Run nuclei
        try:
            self.logger.info(f"Running command: {' '.join(cmd)}")
            subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            self.logger.error(f"Error running nuclei: {e.stderr.decode()}")
            return []
        
        # Parse results
        if not os.path.exists(output_file):
            self.logger.error(f"Output file not found: {output_file}")
            return []
        
        vulnerabilities = []
        
        try:
            with open(output_file, 'r') as f:
                for line in f:
                    try:
                        finding = json.loads(line)
                        
                        vuln = {
                            "type": "nuclei",
                            "name": finding.get('info', {}).get('name', 'Unknown'),
                            "url": finding.get('host', ''),
                            "severity": finding.get('info', {}).get('severity', 'info').lower(),
                            "description": finding.get('info', {}).get('description', ''),
                            "matched": finding.get('matched', ''),
                            "tags": finding.get('info', {}).get('tags', []),
                            "references": finding.get('info', {}).get('reference', []),
                            "cvss_score": finding.get('info', {}).get('classification', {}).get('cvss-score', '')
                        }
                        
                        vulnerabilities.append(vuln)
                    except json.JSONDecodeError:
                        continue
        except Exception as e:
            self.logger.error(f"Error parsing nuclei results: {str(e)}")
        
        self.logger.info(f"Found {len(vulnerabilities)} vulnerabilities with nuclei")
        
        return vulnerabilities
    
    def _run_nikto(self, targets: List[str], output_dir: str, user_agent: str) -> List[Dict[str, Any]]:
        """
        Run nikto for web vulnerability scanning.
        
        Args:
            targets: List of target URLs
            output_dir: Output directory
            user_agent: User agent string
            
        Returns:
            List of vulnerabilities
        """
        vulnerabilities = []
        
        for target in targets:
            self.logger.info(f"Running nikto for {target}")
            
            # Output file
            output_file = os.path.join(output_dir, f"nikto_{urlparse(target).netloc}.json")
            
            # Build nikto command
            cmd = [
                "nikto",
                "-h", target,
                "-o", output_file,
                "-Format", "json"
            ]
            
            # Add user agent if provided
            if user_agent:
                cmd.extend(["-useragent", user_agent])
            
            # Run nikto
            try:
                subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            except subprocess.CalledProcessError as e:
                self.logger.error(f"Error running nikto: {e.stderr.decode()}")
                continue
            
            # Parse results
            if not os.path.exists(output_file):
                self.logger.error(f"Output file not found: {output_file}")
                continue
            
            try:
                with open(output_file, 'r') as f:
                    data = json.load(f)
                
                for item in data.get('vulnerabilities', []):
                    severity = 'low'
                    
                    # Determine severity by keywords
                    if any(kw in item.get('title', '').lower() for kw in ['critical', 'remote code execution', 'rce']):
                        severity = 'critical'
                    elif any(kw in item.get('title', '').lower() for kw in ['high', 'sql injection', 'xss']):
                        severity = 'high'
                    elif any(kw in item.get('title', '').lower() for kw in ['medium', 'csrf']):
                        severity = 'medium'
                    
                    vuln = {
                        "type": "nikto",
                        "name": item.get('title', 'Unknown'),
                        "url": target + item.get('uri', ''),
                        "severity": severity,
                        "description": item.get('message', ''),
                        "osvdb": item.get('osvdb', '')
                    }
                    
                    vulnerabilities.append(vuln)
            except Exception as e:
                self.logger.error(f"Error parsing nikto results: {str(e)}")
        
        self.logger.info(f"Found {len(vulnerabilities)} vulnerabilities with nikto")
        
        return vulnerabilities
    
    def _run_whatweb(self, targets: List[str], output_dir: str, user_agent: str) -> Dict[str, Any]:
        """
        Run whatweb for technology fingerprinting.
        
        Args:
            targets: List of target URLs
            output_dir: Output directory
            user_agent: User agent string
            
        Returns:
            Dict with technology information
        """
        tech_info = {}
        
        for target in targets:
            self.logger.info(f"Running whatweb for {target}")
            
            # Output file
            output_file = os.path.join(output_dir, f"whatweb_{urlparse(target).netloc}.json")
            
            # Build whatweb command
            cmd = [
                "whatweb",
                "--log-json", output_file,
                target
            ]
            
            # Add user agent if provided
            if user_agent:
                cmd.extend(["--user-agent", user_agent])
            
            # Run whatweb
            try:
                subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            except subprocess.CalledProcessError as e:
                self.logger.error(f"Error running whatweb: {e.stderr.decode()}")
                continue
            
            # Parse results
            if not os.path.exists(output_file):
                self.logger.error(f"Output file not found: {output_file}")
                continue
            
            try:
                with open(output_file, 'r') as f:
                    data = json.load(f)
                
                target_info = {}
                
                for item in data:
                    url = item.get('target', '')
                    plugins = item.get('plugins', {})
                    
                    # Extract technology info
                    technologies = []
                    for plugin_name, plugin_data in plugins.items():
                        # Skip non-technology entries
                        if plugin_name in ['HTTPServer', 'Title', 'IP', 'Country', 'Email']:
                            continue
                        
                        # Extract version if available
                        version = ""
                        if isinstance(plugin_data, dict) and 'version' in plugin_data:
                            version = plugin_data['version']
                        
                        technologies.append({
                            "name": plugin_name,
                            "version": version,
                            "confidence": plugin_data.get('confidence', [0])[0] if isinstance(plugin_data, dict) else 0
                        })
                    
                    target_info[url] = {
                        "technologies": technologies,
                        "server": plugins.get('HTTPServer', {}).get('string', [''])[0] if isinstance(plugins.get('HTTPServer', {}), dict) else '',
                        "title": plugins.get('Title', {}).get('string', [''])[0] if isinstance(plugins.get('Title', {}), dict) else ''
                    }
                
                tech_info.update(target_info)
            except Exception as e:
                self.logger.error(f"Error parsing whatweb results: {str(e)}")
        
        return tech_info
    
    def _run_sqlmap_discovery(self, targets: List[str], output_dir: str, user_agent: str, rate_limit: int) -> List[Dict[str, Any]]:
        """
        Run sqlmap in discovery mode to find potential SQL injections.
        
        Args:
            targets: List of target URLs
            output_dir: Output directory
            user_agent: User agent string
            rate_limit: Rate limit
            
        Returns:
            List of vulnerabilities
        """
        vulnerabilities = []
        
        # First, find forms and parameters
        params = []
        
        for target in targets:
            self.logger.info(f"Finding parameters for {target}")
            
            try:
                response = requests.get(target, headers={"User-Agent": user_agent}, timeout=10, verify=False)
                
                # Extract forms
                forms = re.findall(r'<form.*?action=["\']([^"\']*)["\']', response.text, re.DOTALL)
                
                for form in forms:
                    form_url = form
                    if not form.startswith(('http://', 'https://')):
                        # Relative URL
                        form_url = target + ('/' if not form.startswith('/') else '') + form
                    
                    params.append(form_url)
                
                # Extract query parameters from links
                links = re.findall(r'href=["\']([^"\']*\?[^"\']*)["\']', response.text)
                
                for link in links:
                    link_url = link
                    if not link.startswith(('http://', 'https://')):
                        # Relative URL
                        link_url = target + ('/' if not link.startswith('/') else '') + link
                    
                    if '?' in link_url:
                        params.append(link_url)
            except Exception as e:
                self.logger.error(f"Error finding parameters for {target}: {str(e)}")
        
        # Deduplicate parameters
        params = list(set(params))
        
        # Test each parameter with sqlmap
        for param_url in params:
            self.logger.info(f"Running sqlmap for {param_url}")
            
            # Output directory for this URL
            url_hash = hash(param_url) % 10000  # Simple hash to create unique directory
            sqlmap_output_dir = os.path.join(output_dir, f"sqlmap_{url_hash}")
            
            # Build sqlmap command
            cmd = [
                "sqlmap",
                "--batch",
                "--forms",
                "--level", "1",
                "--risk", "1",
                "--delay", str(1/rate_limit),
                "--output-dir", sqlmap_output_dir,
                "--user-agent", user_agent,
                "--url", param_url
            ]
            
            # Run sqlmap
            try:
                subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            except subprocess.CalledProcessError as e:
                self.logger.error(f"Error running sqlmap: {e.stderr.decode()}")
                continue
            
            # Parse results
            target_file = os.path.join(sqlmap_output_dir, urlparse(param_url).netloc, "target.txt")
            if not os.path.exists(target_file):
                continue
            
            log_file = os.path.join(sqlmap_output_dir, urlparse(param_url).netloc, "log")
            if not os.path.exists(log_file):
                continue
            
            # Check if SQL injection was found
            with open(log_file, 'r', errors='ignore') as f:
                log_content = f.read()
                
                if "sqlmap identified the following injection point" in log_content:
                    # Extract details
                    parameter = re.search(r"Parameter: ([^\s]+)", log_content)
                    param_name = parameter.group(1) if parameter else "Unknown"
                    
                    place = re.search(r"Place: ([^\s]+)", log_content)
                    place_value = place.group(1) if place else "Unknown"
                    
                    technique = re.search(r"Technique: ([^\s]+)", log_content)
                    technique_value = technique.group(1) if technique else "Unknown"
                    
                    vuln = {
                        "type": "sqli",
                        "name": f"SQL Injection in {param_name}",
                        "url": param_url,
                        "severity": "high",
                        "description": f"SQL injection vulnerability found in parameter '{param_name}' at {place_value} using {technique_value} technique.",
                        "parameter": param_name,
                        "place": place_value,
                        "technique": technique_value
                    }
                    
                    vulnerabilities.append(vuln)
                    self.logger.info(f"Found SQL injection in {param_url}")
        
        self.logger.info(f"Found {len(vulnerabilities)} SQL injection vulnerabilities")
        
        return vulnerabilities
    
    @classmethod
    def cli_options(cls) -> List[Dict[str, Any]]:
        """Define CLI options for this plugin."""
        return [
            {
                "name": "--mode",
                "help": "Scan mode (basic, standard, thorough)",
                "type": str,
                "default": "standard"
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
                "default": "bbhunt-web-scan"
            },
            {
                "name": "--rate-limit",
                "help": "Rate limit (requests per second)",
                "type": int,
                "default": 10
            },
            {
                "name": "--exclude",
                "help": "Vulnerability types to exclude (comma-separated)",
                "type": str
            }
        ]
    
    @classmethod
    def interactive_options(cls) -> List[Dict[str, Any]]:
        """Define interactive prompts for this plugin."""
        return [
            {
                "type": "input",
                "name": "mode",
                "message": "Scan mode (basic, standard, thorough):",
                "default": "standard"
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
                "default": "bbhunt-web-scan"
            },
            {
                "type": "input",
                "name": "rate_limit",
                "message": "Rate limit (requests per second):",
                "default": "10"
            },
            {
                "type": "input",
                "name": "exclude",
                "message": "Vulnerability types to exclude (comma-separated):",
                "default": ""
            }
        ]
