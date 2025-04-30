#!/usr/bin/env python3
# core/resources.py - Resource management system

import psutil
import os
import json
import subprocess
import time
from dataclasses import dataclass
from typing import Dict, Any, Tuple, Optional, List

@dataclass
class ResourceRequirements:
    """Resource requirements for a plugin or task."""
    memory_mb: int
    cpu_cores: float
    disk_mb: int
    network: bool
    
    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> 'ResourceRequirements':
        """Create ResourceRequirements from a dictionary."""
        return cls(
            memory_mb=parse_memory_to_mb(data.get('memory', '100MB')),
            cpu_cores=float(data.get('cpu', 0.5)),
            disk_mb=parse_memory_to_mb(data.get('disk', '10MB')),
            network=bool(data.get('network', False))
        )

def parse_memory_to_mb(memory_str: str) -> int:
    """Parse memory string like 2G or 500M to MB."""
    memory_str = str(memory_str).upper()
    
    if memory_str.endswith('GB'):
        return int(float(memory_str[:-2]) * 1024)
    elif memory_str.endswith('G'):
        return int(float(memory_str[:-1]) * 1024)
    elif memory_str.endswith('MB'):
        return int(float(memory_str[:-2]))
    elif memory_str.endswith('M'):
        return int(float(memory_str[:-1]))
    elif memory_str.endswith('KB'):
        return int(float(memory_str[:-2]) / 1024)
    elif memory_str.endswith('K'):
        return int(float(memory_str[:-1]) / 1024)
    
    try:
        # Assume it's already in MB
        return int(float(memory_str))
    except ValueError:
        # Default to 100MB if parsing fails
        return 100

class ResourceManager:
    """Manages system resources for bug bounty operations."""
    
    def __init__(self, config: Optional[Dict[str, Any]] = None):
        """
        Initialize resource manager.
        
        Args:
            config: Optional configuration
        """
        self.config = config or {}
        
        # Get resource limits from environment or config
        self.max_memory = parse_memory_to_mb(
            os.environ.get('MAX_MEMORY', 
                          self.config.get('max_memory', '4G')))
        
        self.max_cpu = float(
            os.environ.get('MAX_CPU', 
                          self.config.get('max_cpu', psutil.cpu_count())))
        
        # Check for Docker
        self.docker_available = self._is_docker_available()
        
        # For resource tracking
        self.active_processes = {}
    
    def _is_docker_available(self) -> bool:
        """Check if Docker is available."""
        try:
            result = subprocess.run(
                ['docker', 'info'], 
                stdout=subprocess.PIPE, 
                stderr=subprocess.PIPE,
                check=False
            )
            return result.returncode == 0
        except Exception:
            return False
    
    def check_resources(self, requirements: ResourceRequirements) -> Tuple[bool, str]:
        """
        Check if system has enough resources.
        
        Args:
            requirements: Resource requirements
            
        Returns:
            Tuple of (has_resources, message)
        """
        # Check available memory
        avail_memory_mb = psutil.virtual_memory().available / (1024 * 1024)
        
        if requirements.memory_mb > avail_memory_mb:
            return False, f"Not enough memory. Required: {requirements.memory_mb}MB, Available: {avail_memory_mb:.1f}MB"
        
        # Check available CPU
        avail_cpu = psutil.cpu_count() - psutil.cpu_percent(interval=0.1)/100 * psutil.cpu_count()
        
        if requirements.cpu_cores > avail_cpu:
            return False, f"Not enough CPU. Required: {requirements.cpu_cores} cores, Available: {avail_cpu:.1f} cores"
        
        # Check available disk space
        if requirements.disk_mb > 0:
            disk_usage = psutil.disk_usage(os.getcwd())
            avail_disk_mb = disk_usage.free / (1024 * 1024)
            
            if requirements.disk_mb > avail_disk_mb:
                return False, f"Not enough disk space. Required: {requirements.disk_mb}MB, Available: {avail_disk_mb:.1f}MB"
        
        # Check network connectivity if required
        if requirements.network:
            # Simple connectivity check
            try:
                result = subprocess.run(
                    ['ping', '-c', '1', '8.8.8.8'],
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False
                )
                if result.returncode != 0:
                    return False, "Network connectivity check failed"
            except Exception:
                return False, "Network connectivity check failed"
        
        return True, "Sufficient resources available"
    
    def run_in_container(self, image: str, command: List[str], 
                        volumes: Dict[str, str], 
                        environment: Dict[str, str] = None,
                        resource_limits: Dict[str, Any] = None) -> str:
        """
        Run a command in a Docker container.
        
        Args:
            image: Docker image to use
            command: Command to run
            volumes: Volumes to mount (host_path -> container_path)
            environment: Environment variables
            resource_limits: Resource limits (memory, cpu)
            
        Returns:
            Container ID
        """
        if not self.docker_available:
            raise RuntimeError("Docker is not available")
        
        # Build docker run command
        docker_cmd = ['docker', 'run', '-d']
        
        # Add volumes
        for host_path, container_path in volumes.items():
            docker_cmd.extend(['-v', f"{host_path}:{container_path}"])
        
        # Add environment variables
        if environment:
            for key, value in environment.items():
                docker_cmd.extend(['-e', f"{key}={value}"])
        
        # Add resource limits
        if resource_limits:
            if 'memory' in resource_limits:
                docker_cmd.extend(['--memory', str(resource_limits['memory'])])
            if 'cpu' in resource_limits:
                docker_cmd.extend(['--cpus', str(resource_limits['cpu'])])
        
        # Add image and command
        docker_cmd.append(image)
        docker_cmd.extend(command)
        
        # Run container
        result = subprocess.run(
            docker_cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            text=True
        )
        
        if result.returncode != 0:
            raise RuntimeError(f"Failed to start container: {result.stderr}")
        
        # Return container ID
        return result.stdout.strip()
    
    def get_container_status(self, container_id: str) -> Dict[str, Any]:
        """
        Get status of a running container.
        
        Args:
            container_id: Container ID
            
        Returns:
            Container status
        """
        if not self.docker_available:
            raise RuntimeError("Docker is not available")
        
        # Check if container exists
        result = subprocess.run(
            ['docker', 'inspect', container_id],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            text=True
        )
        
        if result.returncode != 0:
            return {"status": "not_found", "logs": "", "exit_code": None}
        
        # Parse container info
        container_info = json.loads(result.stdout)[0]
        
        # Get container status
        status = container_info['State']['Status']
        
        # Get exit code if container has stopped
        exit_code = None
        if status in ('exited', 'dead'):
            exit_code = container_info['State']['ExitCode']
        
        # Get logs
        logs_result = subprocess.run(
            ['docker', 'logs', '--tail', '10', container_id],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            text=True
        )
        
        logs = logs_result.stdout if logs_result.returncode == 0 else ""
        
        return {
            "status": status,
            "logs": logs,
            "exit_code": exit_code
        }
    
    def stop_container(self, container_id: str) -> bool:
        """
        Stop a running container.
        
        Args:
            container_id: Container ID
            
        Returns:
            True if successful, False otherwise
        """
        if not self.docker_available:
            raise RuntimeError("Docker is not available")
        
        # Stop container
        result = subprocess.run(
            ['docker', 'stop', container_id],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False
        )
        
        return result.returncode == 0
    
    def track_process(self, process_id: int, name: str, resources: ResourceRequirements) -> None:
        """
        Track a running process.
        
        Args:
            process_id: Process ID
            name: Process name
            resources: Resource requirements
        """
        self.active_processes[process_id] = {
            'name': name,
            'resources': resources,
            'start_time': time.time()
        }
    
    def untrack_process(self, process_id: int) -> None:
        """
        Stop tracking a process.
        
        Args:
            process_id: Process ID
        """
        if process_id in self.active_processes:
            del self.active_processes[process_id]
    
    def get_resource_usage(self) -> Dict[str, Any]:
        """
        Get current resource usage.
        
        Returns:
            Dict with resource usage info
        """
        # System memory usage
        memory = psutil.virtual_memory()
        memory_usage = {
            'total': memory.total / (1024 * 1024),  # MB
            'available': memory.available / (1024 * 1024),  # MB
            'used': memory.used / (1024 * 1024),  # MB
            'percent': memory.percent
        }
        
        # System CPU usage
        cpu_usage = {
            'percent': psutil.cpu_percent(),
            'cores': psutil.cpu_count()
        }
        
        # Disk usage
        disk = psutil.disk_usage(os.getcwd())
        disk_usage = {
            'total': disk.total / (1024 * 1024),  # MB
            'free': disk.free / (1024 * 1024),  # MB
            'used': disk.used / (1024 * 1024),  # MB
            'percent': disk.percent
        }
        
        # Active processes
        processes = {}
        for pid, info in list(self.active_processes.items()):
            # Check if process is still running
            try:
                process = psutil.Process(pid)
                
                # Get process resource usage
                with process.oneshot():
                    processes[pid] = {
                        'name': info['name'],
                        'memory_mb': process.memory_info().rss / (1024 * 1024),
                        'cpu_percent': process.cpu_percent(),
                        'runtime': time.time() - info['start_time']
                    }
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                # Process no longer exists or cannot be accessed
                self.untrack_process(pid)
        
        return {
            'memory': memory_usage,
            'cpu': cpu_usage,
            'disk': disk_usage,
            'processes': processes
        }
