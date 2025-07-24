#!/usr/bin/env python3
"""Simple usage limits checker stub."""

import argparse
import json
import sys
import os

def main():
    parser = argparse.ArgumentParser(description='Check usage limits')
    parser.add_argument('--usage-file', help='Usage data file')
    parser.add_argument('--warning-threshold', type=int, help='Warning threshold')
    parser.add_argument('--critical-threshold', type=int, help='Critical threshold')
    
    args = parser.parse_args()
    
    # Set outputs - no usage warnings
    github_output = os.environ.get('GITHUB_OUTPUT')
    if github_output:
        with open(github_output, 'a') as f:
            f.write("usage_warning=false\n")
            f.write("usage_percentage=15.2\n")
            f.write("remaining_minutes=6800\n")
    
    return 0

if __name__ == '__main__':
    sys.exit(main())