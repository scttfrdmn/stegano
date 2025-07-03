#!/usr/bin/env python3
"""
CI/CD Flow Monitoring Script for stegano project

This script monitors the status of CI/CD workflows and provides comprehensive
reporting on build health, performance, and issues.
"""

import os
import sys
import json
import requests
import argparse
from datetime import datetime, timedelta
from typing import Dict, List, Optional, Any
import time

class GitHubCIMonitor:
    def __init__(self, token: str, repo: str, owner: str):
        self.token = token
        self.repo = repo
        self.owner = owner
        self.base_url = "https://api.github.com"
        self.headers = {
            "Authorization": f"token {token}",
            "Accept": "application/vnd.github.v3+json"
        }
    
    def get_workflow_runs(self, days: int = 7) -> List[Dict]:
        """Get workflow runs from the last N days"""
        since = (datetime.utcnow() - timedelta(days=days)).isoformat() + "Z"
        
        url = f"{self.base_url}/repos/{self.owner}/{self.repo}/actions/runs"
        params = {
            "per_page": 100,
            "created": f">{since}"
        }
        
        all_runs = []
        page = 1
        
        while True:
            params["page"] = page
            response = requests.get(url, headers=self.headers, params=params)
            
            if response.status_code != 200:
                print(f"Error fetching workflow runs: {response.status_code}")
                break
                
            data = response.json()
            runs = data.get("workflow_runs", [])
            
            if not runs:
                break
                
            all_runs.extend(runs)
            page += 1
            
            # Avoid rate limiting
            time.sleep(0.1)
            
        return all_runs
    
    def get_workflow_details(self, workflow_id: int) -> Optional[Dict]:
        """Get detailed information about a specific workflow"""
        url = f"{self.base_url}/repos/{self.owner}/{self.repo}/actions/workflows/{workflow_id}"
        response = requests.get(url, headers=self.headers)
        
        if response.status_code == 200:
            return response.json()
        return None
    
    def get_branch_protection_status(self, branch: str) -> Dict:
        """Check branch protection rules and their compliance"""
        url = f"{self.base_url}/repos/{self.owner}/{self.repo}/branches/{branch}/protection"
        response = requests.get(url, headers=self.headers)
        
        if response.status_code == 200:
            return response.json()
        elif response.status_code == 404:
            return {"protected": False}
        else:
            return {"error": f"HTTP {response.status_code}"}
    
    def analyze_workflow_health(self, runs: List[Dict]) -> Dict:
        """Analyze overall workflow health metrics"""
        if not runs:
            return {
                "overall_status": "unknown",
                "total_runs": 0,
                "success_rate": 0,
                "avg_duration": 0,
                "issues": []
            }
        
        total_runs = len(runs)
        successful_runs = len([r for r in runs if r["conclusion"] == "success"])
        failed_runs = len([r for r in runs if r["conclusion"] == "failure"])
        cancelled_runs = len([r for r in runs if r["conclusion"] == "cancelled"])
        
        success_rate = (successful_runs / total_runs) * 100 if total_runs > 0 else 0
        
        # Calculate average duration for completed runs
        completed_runs = [r for r in runs if r["conclusion"] in ["success", "failure"]]
        durations = []
        
        for run in completed_runs:
            if run["created_at"] and run["updated_at"]:
                created = datetime.fromisoformat(run["created_at"].replace("Z", "+00:00"))
                updated = datetime.fromisoformat(run["updated_at"].replace("Z", "+00:00"))
                duration = (updated - created).total_seconds()
                durations.append(duration)
        
        avg_duration = sum(durations) / len(durations) if durations else 0
        
        # Determine overall status
        if success_rate >= 95:
            overall_status = "healthy"
        elif success_rate >= 80:
            overall_status = "warning"
        else:
            overall_status = "critical"
        
        # Identify issues
        issues = []
        
        if success_rate < 80:
            issues.append({
                "type": "low_success_rate",
                "severity": "high",
                "message": f"Success rate is {success_rate:.1f}% (below 80% threshold)"
            })
        
        if failed_runs > 5:
            issues.append({
                "type": "high_failure_count",
                "severity": "medium",
                "message": f"{failed_runs} failed runs in the last week"
            })
        
        if avg_duration > 1800:  # 30 minutes
            issues.append({
                "type": "slow_builds",
                "severity": "low",
                "message": f"Average build time is {avg_duration/60:.1f} minutes"
            })
        
        # Check for recent failures
        recent_runs = sorted(runs, key=lambda x: x["created_at"], reverse=True)[:10]
        recent_failures = [r for r in recent_runs if r["conclusion"] == "failure"]
        
        if len(recent_failures) >= 3:
            issues.append({
                "type": "recent_failures",
                "severity": "high",
                "message": f"{len(recent_failures)} failures in last 10 runs"
            })
        
        return {
            "overall_status": overall_status,
            "total_runs": total_runs,
            "successful_runs": successful_runs,
            "failed_runs": failed_runs,
            "cancelled_runs": cancelled_runs,
            "success_rate": success_rate,
            "avg_duration": avg_duration,
            "issues": issues,
            "recent_failures": recent_failures[:3]  # Last 3 failures for context
        }
    
    def analyze_branch_health(self, runs: List[Dict]) -> Dict:
        """Analyze health per branch"""
        branch_stats = {}
        
        for run in runs:
            branch = run["head_branch"]
            if branch not in branch_stats:
                branch_stats[branch] = {
                    "total_runs": 0,
                    "successful_runs": 0,
                    "failed_runs": 0,
                    "last_run": None,
                    "last_success": None
                }
            
            stats = branch_stats[branch]
            stats["total_runs"] += 1
            
            if run["conclusion"] == "success":
                stats["successful_runs"] += 1
                if not stats["last_success"] or run["created_at"] > stats["last_success"]:
                    stats["last_success"] = run["created_at"]
            elif run["conclusion"] == "failure":
                stats["failed_runs"] += 1
            
            if not stats["last_run"] or run["created_at"] > stats["last_run"]:
                stats["last_run"] = run["created_at"]
        
        # Calculate success rates and identify problematic branches
        for branch, stats in branch_stats.items():
            if stats["total_runs"] > 0:
                stats["success_rate"] = (stats["successful_runs"] / stats["total_runs"]) * 100
            else:
                stats["success_rate"] = 0
            
            # Determine branch health
            if stats["success_rate"] >= 90:
                stats["health"] = "healthy"
            elif stats["success_rate"] >= 70:
                stats["health"] = "warning"
            else:
                stats["health"] = "critical"
        
        return branch_stats
    
    def get_failing_workflows(self, runs: List[Dict]) -> List[Dict]:
        """Get details about currently failing workflows"""
        failing_workflows = {}
        
        for run in runs:
            if run["conclusion"] == "failure":
                workflow_name = run["name"]
                if workflow_name not in failing_workflows:
                    failing_workflows[workflow_name] = {
                        "name": workflow_name,
                        "failure_count": 0,
                        "recent_failures": [],
                        "first_failure": run["created_at"],
                        "last_failure": run["created_at"]
                    }
                
                workflow = failing_workflows[workflow_name]
                workflow["failure_count"] += 1
                workflow["recent_failures"].append({
                    "run_id": run["id"],
                    "branch": run["head_branch"],
                    "created_at": run["created_at"],
                    "html_url": run["html_url"]
                })
                
                if run["created_at"] > workflow["last_failure"]:
                    workflow["last_failure"] = run["created_at"]
                if run["created_at"] < workflow["first_failure"]:
                    workflow["first_failure"] = run["created_at"]
        
        # Sort by failure count and recency
        failing_list = list(failing_workflows.values())
        failing_list.sort(key=lambda x: (x["failure_count"], x["last_failure"]), reverse=True)
        
        return failing_list
    
    def check_dependencies(self) -> Dict:
        """Check for dependency-related issues"""
        # This would integrate with Dependabot or similar tools
        # For now, return a placeholder
        return {
            "outdated_dependencies": 0,
            "security_vulnerabilities": 0,
            "last_update": None
        }

def main():
    parser = argparse.ArgumentParser(description="Monitor CI/CD workflow status")
    parser.add_argument("--format", choices=["json", "github-actions"], default="json",
                       help="Output format")
    parser.add_argument("--days", type=int, default=7,
                       help="Number of days to analyze")
    parser.add_argument("--output", help="Output file path")
    
    args = parser.parse_args()
    
    # Get configuration from environment
    token = os.getenv("GITHUB_TOKEN")
    repo = os.getenv("GITHUB_REPOSITORY", "stegano").split("/")[-1]
    owner = os.getenv("GITHUB_REPOSITORY_OWNER", "scttfrdmn")
    notification_level = os.getenv("NOTIFICATION_LEVEL", "all")
    
    if not token:
        print("Error: GITHUB_TOKEN environment variable is required")
        sys.exit(1)
    
    # Initialize monitor
    monitor = GitHubCIMonitor(token, repo, owner)
    
    print(f"🔍 Monitoring CI/CD status for {owner}/{repo}...")
    
    # Get workflow runs
    runs = monitor.get_workflow_runs(days=args.days)
    print(f"Found {len(runs)} workflow runs in the last {args.days} days")
    
    # Analyze health
    health = monitor.analyze_workflow_health(runs)
    branch_health = monitor.analyze_branch_health(runs)
    failing_workflows = monitor.get_failing_workflows(runs)
    dependencies = monitor.check_dependencies()
    
    # Create comprehensive status report
    status_report = {
        "timestamp": datetime.utcnow().isoformat() + "Z",
        "repository": f"{owner}/{repo}",
        "analysis_period_days": args.days,
        "overall_health": health,
        "branch_health": branch_health,
        "failing_workflows": failing_workflows,
        "dependencies": dependencies,
        "summary": {
            "total_runs": health["total_runs"],
            "success_rate": health["success_rate"],
            "critical_issues": len([i for i in health["issues"] if i["severity"] == "high"]),
            "has_issues": len(health["issues"]) > 0,
            "has_critical_issues": any(i["severity"] == "high" for i in health["issues"]),
            "persistent_failures": len(failing_workflows) > 0 and any(
                len(fw["recent_failures"]) >= 3 for fw in failing_workflows
            )
        }
    }
    
    # Determine status color for notifications
    if health["overall_status"] == "healthy":
        status_color = "good"
    elif health["overall_status"] == "warning":
        status_color = "warning"
    else:
        status_color = "danger"
    
    # Output for GitHub Actions
    if args.format == "github-actions":
        print(f"::set-output name=overall_status::{health['overall_status']}")
        print(f"::set-output name=success_rate::{health['success_rate']:.1f}%")
        print(f"::set-output name=failed_count::{health['failed_runs']}")
        print(f"::set-output name=has_issues::{str(len(health['issues']) > 0).lower()}")
        print(f"::set-output name=has_critical_issues::{str(any(i['severity'] == 'high' for i in health['issues'])).lower()}")
        print(f"::set-output name=status_color::{status_color}")
        print(f"::set-output name=persistent_failures::{str(status_report['summary']['persistent_failures']).lower()}")
        
        # Branch status summary
        unhealthy_branches = [b for b, s in branch_health.items() if s.get("health") != "healthy"]
        branch_status = f"{len(branch_health)} branches monitored"
        if unhealthy_branches:
            branch_status += f", {len(unhealthy_branches)} need attention"
        print(f"::set-output name=branch_status::{branch_status}")
    
    # Save full report
    output_file = args.output or "ci-status.json"
    with open(output_file, "w") as f:
        json.dump(status_report, f, indent=2)
    
    print(f"✅ Status report saved to {output_file}")
    
    # Print summary
    print(f"\n📊 CI/CD Health Summary:")
    print(f"Overall Status: {health['overall_status'].upper()}")
    print(f"Success Rate: {health['success_rate']:.1f}%")
    print(f"Failed Runs: {health['failed_runs']}")
    print(f"Issues Found: {len(health['issues'])}")
    
    if health["issues"]:
        print(f"\n⚠️ Issues Detected:")
        for issue in health["issues"]:
            severity_emoji = {"high": "🚨", "medium": "⚠️", "low": "💡"}
            print(f"  {severity_emoji[issue['severity']]} {issue['message']}")
    
    if failing_workflows:
        print(f"\n❌ Failing Workflows:")
        for fw in failing_workflows[:3]:  # Show top 3
            print(f"  • {fw['name']}: {fw['failure_count']} failures")
    
    # Exit with appropriate code for monitoring systems
    if status_report["summary"]["has_critical_issues"]:
        sys.exit(2)  # Critical issues
    elif status_report["summary"]["has_issues"]:
        sys.exit(1)  # Warning issues
    else:
        sys.exit(0)  # All good

if __name__ == "__main__":
    main()