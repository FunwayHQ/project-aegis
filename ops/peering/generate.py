#!/usr/bin/env python3
"""
AEGIS Peering Manager - BGP Configuration Generator

Generates BIRD v2 configuration files from structured YAML data using Jinja2 templates.
Supports multi-site deployment with per-node customization.

Usage:
    python3 generate.py --node edge-us-east --output /etc/bird/bird.conf
    python3 generate.py --all --output-dir ./output/
    python3 generate.py --validate --node edge-us-east
"""

import argparse
import os
import sys
import subprocess
from pathlib import Path
from typing import Dict, Any, List, Optional

try:
    import yaml
except ImportError:
    print("Error: PyYAML is required. Install with: pip3 install pyyaml")
    sys.exit(1)

try:
    from jinja2 import Environment, FileSystemLoader, select_autoescape
except ImportError:
    print("Error: Jinja2 is required. Install with: pip3 install jinja2")
    sys.exit(1)


class PeeringManager:
    """Manages BGP peering configuration generation."""

    def __init__(self, base_dir: Optional[Path] = None):
        """Initialize the peering manager.

        Args:
            base_dir: Base directory containing templates/ and data/
        """
        self.base_dir = base_dir or Path(__file__).parent
        self.templates_dir = self.base_dir / "templates"
        self.data_dir = self.base_dir / "data"
        self.output_dir = self.base_dir / "output"

        # Validate directory structure
        self._validate_directories()

        # Initialize Jinja2 environment
        self.env = Environment(
            loader=FileSystemLoader(str(self.templates_dir)),
            autoescape=select_autoescape(['html', 'xml']),
            trim_blocks=True,
            lstrip_blocks=True,
        )

        # Add custom filters
        self.env.filters['quote'] = lambda s: f'"{s}"'
        self.env.filters['as_list'] = lambda s: s if isinstance(s, list) else [s]

    def _validate_directories(self) -> None:
        """Validate required directories exist."""
        required_dirs = [
            self.templates_dir,
            self.data_dir,
            self.data_dir / "peers",
            self.data_dir / "nodes",
        ]

        for d in required_dirs:
            if not d.exists():
                raise FileNotFoundError(f"Required directory not found: {d}")

    def load_yaml(self, path: Path) -> Dict[str, Any]:
        """Load and parse a YAML file.

        Args:
            path: Path to YAML file

        Returns:
            Parsed YAML content as dictionary
        """
        if not path.exists():
            raise FileNotFoundError(f"YAML file not found: {path}")

        with open(path, 'r') as f:
            return yaml.safe_load(f) or {}

    def load_global_config(self) -> Dict[str, Any]:
        """Load global configuration.

        Returns:
            Global configuration dictionary
        """
        global_path = self.data_dir / "global.yaml"
        if not global_path.exists():
            raise FileNotFoundError(
                f"Global configuration not found: {global_path}\n"
                "Create data/global.yaml with AS number, anycast prefixes, etc."
            )
        return self.load_yaml(global_path)

    def load_peer(self, peer_name: str) -> Dict[str, Any]:
        """Load a peer configuration.

        Args:
            peer_name: Name of the peer (filename without .yaml)

        Returns:
            Peer configuration dictionary
        """
        peer_path = self.data_dir / "peers" / f"{peer_name}.yaml"
        return self.load_yaml(peer_path)

    def load_node(self, node_name: str) -> Dict[str, Any]:
        """Load a node configuration.

        Args:
            node_name: Name of the node (filename without .yaml)

        Returns:
            Node configuration dictionary
        """
        node_path = self.data_dir / "nodes" / f"{node_name}.yaml"
        return self.load_yaml(node_path)

    def list_nodes(self) -> List[str]:
        """List all available node configurations.

        Returns:
            List of node names
        """
        nodes_dir = self.data_dir / "nodes"
        return [f.stem for f in nodes_dir.glob("*.yaml")]

    def list_peers(self) -> List[str]:
        """List all available peer configurations.

        Returns:
            List of peer names
        """
        peers_dir = self.data_dir / "peers"
        return [f.stem for f in peers_dir.glob("*.yaml")]

    def build_context(self, node_name: str) -> Dict[str, Any]:
        """Build the template rendering context for a node.

        Args:
            node_name: Name of the node

        Returns:
            Context dictionary for template rendering
        """
        # Load configurations
        global_config = self.load_global_config()
        node_config = self.load_node(node_name)

        # Load all peers for this node
        peer_names = node_config.get('peers', [])
        peers = []

        for peer_name in peer_names:
            try:
                peer = self.load_peer(peer_name)

                # Apply node-specific overrides
                overrides = node_config.get('overrides', {}).get(peer_name, {})
                peer.update(overrides)

                # Only include enabled peers
                if peer.get('enabled', True):
                    peers.append(peer)
            except FileNotFoundError:
                print(f"Warning: Peer configuration not found: {peer_name}")

        # Build context
        context = {
            'global': global_config,
            'node': node_config,
            'peers': peers,
            'generated_by': 'AEGIS Peering Manager',
            'generated_at': __import__('datetime').datetime.now(__import__('datetime').timezone.utc).isoformat(),
        }

        return context

    def render_config(self, node_name: str) -> str:
        """Render the BIRD configuration for a node.

        Args:
            node_name: Name of the node

        Returns:
            Rendered BIRD configuration string
        """
        context = self.build_context(node_name)
        template = self.env.get_template("bird.conf.j2")
        return template.render(**context)

    def generate(self, node_name: str, output_path: Optional[Path] = None) -> str:
        """Generate BIRD configuration for a node.

        Args:
            node_name: Name of the node
            output_path: Optional path to write configuration

        Returns:
            Rendered configuration string
        """
        config = self.render_config(node_name)

        if output_path:
            output_path = Path(output_path)
            output_path.parent.mkdir(parents=True, exist_ok=True)
            with open(output_path, 'w') as f:
                f.write(config)
            print(f"Generated: {output_path}")

        return config

    def generate_all(self, output_dir: Optional[Path] = None) -> Dict[str, str]:
        """Generate configurations for all nodes.

        Args:
            output_dir: Directory to write configurations

        Returns:
            Dictionary mapping node names to configurations
        """
        output_dir = Path(output_dir) if output_dir else self.output_dir
        configs = {}

        for node_name in self.list_nodes():
            try:
                output_path = output_dir / node_name / "bird.conf"
                configs[node_name] = self.generate(node_name, output_path)
            except Exception as e:
                print(f"Error generating config for {node_name}: {e}")

        return configs

    def validate_config(self, config: str) -> bool:
        """Validate a BIRD configuration using bird -p.

        Args:
            config: Configuration string to validate

        Returns:
            True if valid, False otherwise
        """
        import tempfile

        # Write config to temp file
        with tempfile.NamedTemporaryFile(mode='w', suffix='.conf', delete=False) as f:
            f.write(config)
            temp_path = f.name

        try:
            # Try to validate with bird
            result = subprocess.run(
                ['bird', '-p', '-c', temp_path],
                capture_output=True,
                text=True,
                timeout=30
            )

            if result.returncode == 0:
                print("Configuration is valid")
                return True
            else:
                print(f"Configuration validation failed:\n{result.stderr}")
                return False

        except FileNotFoundError:
            print("Warning: BIRD not installed, skipping validation")
            print("Install BIRD to enable config validation: apt install bird2")
            return True  # Assume valid if bird not available
        except subprocess.TimeoutExpired:
            print("Warning: BIRD validation timed out")
            return False
        finally:
            os.unlink(temp_path)


def main():
    parser = argparse.ArgumentParser(
        description='AEGIS Peering Manager - BGP Configuration Generator',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
    # Generate config for a specific node
    python3 generate.py --node edge-us-east --output /etc/bird/bird.conf

    # Generate configs for all nodes
    python3 generate.py --all --output-dir ./output/

    # Validate a node's configuration
    python3 generate.py --validate --node edge-us-east

    # List available nodes and peers
    python3 generate.py --list
        """
    )

    parser.add_argument(
        '--node', '-n',
        help='Node name to generate configuration for'
    )
    parser.add_argument(
        '--all', '-a',
        action='store_true',
        help='Generate configurations for all nodes'
    )
    parser.add_argument(
        '--output', '-o',
        help='Output file path (for single node)'
    )
    parser.add_argument(
        '--output-dir', '-d',
        default='./output',
        help='Output directory (for all nodes)'
    )
    parser.add_argument(
        '--validate', '-v',
        action='store_true',
        help='Validate generated configuration with BIRD'
    )
    parser.add_argument(
        '--list', '-l',
        action='store_true',
        help='List available nodes and peers'
    )
    parser.add_argument(
        '--base-dir', '-b',
        help='Base directory containing templates/ and data/'
    )
    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='Print configuration to stdout without writing files'
    )

    args = parser.parse_args()

    try:
        base_dir = Path(args.base_dir) if args.base_dir else None
        manager = PeeringManager(base_dir)

        if args.list:
            print("Available nodes:")
            for node in manager.list_nodes():
                print(f"  - {node}")
            print("\nAvailable peers:")
            for peer in manager.list_peers():
                print(f"  - {peer}")
            return

        if args.all:
            # Generate for all nodes
            output_dir = Path(args.output_dir)
            configs = manager.generate_all(output_dir if not args.dry_run else None)

            if args.dry_run:
                for node_name, config in configs.items():
                    print(f"\n{'='*60}")
                    print(f"# Configuration for: {node_name}")
                    print('='*60)
                    print(config)

            if args.validate:
                print("\nValidating configurations...")
                for node_name, config in configs.items():
                    print(f"\nValidating {node_name}:")
                    manager.validate_config(config)

        elif args.node:
            # Generate for single node
            output_path = Path(args.output) if args.output and not args.dry_run else None
            config = manager.generate(args.node, output_path)

            if args.dry_run or not args.output:
                print(config)

            if args.validate:
                manager.validate_config(config)
        else:
            parser.print_help()
            sys.exit(1)

    except FileNotFoundError as e:
        print(f"Error: {e}")
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == '__main__':
    main()
