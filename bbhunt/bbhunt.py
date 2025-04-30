#!/usr/bin/env python3
# bbhunt.py - Main entry point for the bug bounty hunting framework

import os
import sys
import logging
from typing import Dict, Any, Optional

# Set up logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.StreamHandler(sys.stdout)
    ]
)

logger = logging.getLogger("bbhunt")

def main():
    """Main entry point."""
    # Ensure necessary directories exist
    os.makedirs('data', exist_ok=True)
    os.makedirs('config', exist_ok=True)
    
    # Get mode from environment
    mode = os.environ.get('BBHUNT_MODE', 'cli')
    
    logger.info(f"Starting in {mode} mode")
    
    if mode == 'cli':
        # CLI mode
        from bbhunt.core.cli import cli
        sys.exit(cli())
    elif mode == 'api':
        # API server mode
        try:
            from bbhunt.core.api import app
            import uvicorn
            
            port = int(os.environ.get('BBHUNT_API_PORT', 8080))
            uvicorn.run(app, host="0.0.0.0", port=port)
        except ImportError:
            logger.error("Missing requirements for API mode. Install with 'pip install fastapi uvicorn'")
            sys.exit(1)
    elif mode == 'worker':
        # Worker mode
        try:
            worker_type = os.environ.get('BBHUNT_WORKER_TYPE', 'recon')
            redis_host = os.environ.get('BBHUNT_REDIS_HOST', 'localhost')
            redis_port = int(os.environ.get('BBHUNT_REDIS_PORT', 6379))
            
            from bbhunt.core.worker import Worker
            worker = Worker(worker_type, redis_host, redis_port)
            worker.start()
        except ImportError:
            logger.error("Missing requirements for worker mode. Install with 'pip install redis'")
            sys.exit(1)
    elif mode == 'standalone':
        # Standalone mode - run a specific plugin once
        plugin_name = sys.argv[1] if len(sys.argv) > 1 else None
        target = None
        options = {}
        
        # Parse environment variables for options
        for key, value in os.environ.items():
            if key.startswith('BBHUNT_OPT_'):
                option_key = key[len('BBHUNT_OPT_'):].lower()
                options[option_key] = value
        
        # Parse command line arguments
        i = 1
        while i < len(sys.argv):
            if sys.argv[i] == '--target' or sys.argv[i] == '-t':
                if i + 1 < len(sys.argv):
                    target = sys.argv[i + 1]
                    i += 2
                else:
                    logger.error("Missing target value")
                    sys.exit(1)
            elif sys.argv[i] == '--options' or sys.argv[i] == '-o':
                if i + 1 < len(sys.argv):
                    try:
                        import json
                        options = json.loads(sys.argv[i + 1])
                        i += 2
                    except Exception as e:
                        logger.error(f"Invalid options JSON: {str(e)}")
                        sys.exit(1)
                else:
                    logger.error("Missing options value")
                    sys.exit(1)
            else:
                # First non-option argument is the plugin name
                if not plugin_name:
                    plugin_name = sys.argv[i]
                i += 1
        
        if not plugin_name:
            logger.error("No plugin specified")
            sys.exit(1)
        
        logger.info(f"Running plugin {plugin_name} on {target or 'utility mode'}")
        
        # Load and run the plugin
        from bbhunt.core.plugin import get_plugin
        plugin_class = get_plugin(plugin_name)
        
        if not plugin_class:
            logger.error(f"Plugin {plugin_name} not found")
            sys.exit(1)
        
        plugin = plugin_class()
        plugin.setup()
        
        try:
            result = plugin.execute(target, options)
            logger.info(f"Plugin result: {result['status']}")
            if result['status'] == 'success':
                sys.exit(0)
            else:
                logger.error(f"Plugin error: {result.get('message', 'Unknown error')}")
                sys.exit(1)
        except Exception as e:
            logger.error(f"Error running plugin: {str(e)}")
            sys.exit(1)
        finally:
            plugin.cleanup()
    else:
        logger.error(f"Unknown mode: {mode}")
        sys.exit(1)

if __name__ == '__main__':
    main()
