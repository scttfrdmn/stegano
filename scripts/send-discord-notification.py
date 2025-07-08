#!/usr/bin/env python3
"""
Discord notification sender for CI/CD monitoring
"""

import json
import requests
import argparse
from datetime import datetime
from typing import Dict, Any

class DiscordNotifier:
    def __init__(self, webhook_url: str):
        self.webhook_url = webhook_url
    
    def send_status_notification(self, status_data: Dict[str, Any]) -> bool:
        """Send a rich Discord notification with CI/CD status"""
        
        health = status_data["overall_health"]
        summary = status_data["summary"]
        
        # Determine embed color
        color_map = {
            "healthy": 0x28a745,    # Green
            "warning": 0xffc107,    # Yellow
            "critical": 0xdc3545,   # Red
        }
        embed_color = color_map.get(health["overall_status"], 0x6c757d)
        
        # Status emoji
        status_emoji = {
            "healthy": "✅",
            "warning": "⚠️",
            "critical": "🚨"
        }
        
        # Create embed
        embed = {
            "title": f"{status_emoji.get(health['overall_status'], '❓')} CI/CD Status Report",
            "description": f"**Repository**: {status_data['repository']}",
            "color": embed_color,
            "timestamp": status_data["timestamp"],
            "fields": [
                {
                    "name": "📊 Overall Status",
                    "value": health["overall_status"].title(),
                    "inline": True
                },
                {
                    "name": "✅ Success Rate",
                    "value": f"{health['success_rate']:.1f}%",
                    "inline": True
                },
                {
                    "name": "❌ Failed Runs",
                    "value": str(health["failed_runs"]),
                    "inline": True
                },
                {
                    "name": "🏃 Total Runs",
                    "value": str(health["total_runs"]),
                    "inline": True
                },
                {
                    "name": "⏱️ Avg Duration",
                    "value": f"{health['avg_duration']/60:.1f} minutes",
                    "inline": True
                },
                {
                    "name": "⚠️ Active Issues",
                    "value": str(len(health["issues"])),
                    "inline": True
                }
            ],
            "footer": {
                "text": f"Analysis period: {status_data['analysis_period_days']} days • stegano CI/CD Monitor"
            }
        }
        
        # Add issues field if there are any
        if health["issues"]:
            issues_text = ""
            for issue in health["issues"][:3]:  # Limit to 3 for space
                severity_emoji = {"high": "🚨", "medium": "⚠️", "low": "💡"}
                issues_text += f"{severity_emoji[issue['severity']]} {issue['message']}\n"
            
            if len(health["issues"]) > 3:
                issues_text += f"... and {len(health['issues']) - 3} more issues"
            
            embed["fields"].append({
                "name": "🔍 Issues Detected",
                "value": issues_text,
                "inline": False
            })
        
        # Add failing workflows if any
        if status_data["failing_workflows"]:
            workflows_text = ""
            for fw in status_data["failing_workflows"][:3]:  # Limit to 3
                workflows_text += f"• **{fw['name']}**: {fw['failure_count']} failures\n"
            
            if len(status_data["failing_workflows"]) > 3:
                workflows_text += f"... and {len(status_data['failing_workflows']) - 3} more workflows"
            
            embed["fields"].append({
                "name": "❌ Failing Workflows",
                "value": workflows_text,
                "inline": False
            })
        
        # Add branch health summary
        if status_data["branch_health"]:
            total_branches = len(status_data["branch_health"])
            unhealthy_branches = sum(1 for stats in status_data["branch_health"].values() 
                                   if stats.get("health") != "healthy")
            
            branch_text = f"**{total_branches}** branches monitored"
            if unhealthy_branches > 0:
                branch_text += f"\n**{unhealthy_branches}** need attention"
            
            embed["fields"].append({
                "name": "🌿 Branch Health",
                "value": branch_text,
                "inline": True
            })
        
        # Create payload
        payload = {
            "embeds": [embed]
        }
        
        # Add content for critical issues
        if summary["has_critical_issues"]:
            payload["content"] = f"🚨 **CRITICAL CI/CD ISSUES DETECTED** 🚨\n" \
                               f"Repository: {status_data['repository']}\n" \
                               f"Critical issues require immediate attention!"
        
        # Send notification
        try:
            response = requests.post(self.webhook_url, json=payload)
            response.raise_for_status()
            return True
        except requests.exceptions.RequestException as e:
            print(f"Failed to send Discord notification: {e}")
            return False
    
    def send_workflow_failure_alert(self, workflow_name: str, failure_details: Dict) -> bool:
        """Send a specific workflow failure alert"""
        
        embed = {
            "title": "🚨 Workflow Failure Alert",
            "description": f"**Workflow**: {workflow_name}",
            "color": 0xdc3545,  # Red
            "timestamp": datetime.utcnow().isoformat(),
            "fields": [
                {
                    "name": "Branch",
                    "value": failure_details.get("branch", "unknown"),
                    "inline": True
                },
                {
                    "name": "Run ID",
                    "value": str(failure_details.get("run_id", "unknown")),
                    "inline": True
                },
                {
                    "name": "Failed At",
                    "value": failure_details.get("created_at", "unknown"),
                    "inline": True
                }
            ],
            "footer": {
                "text": "stegano CI/CD Monitor"
            }
        }
        
        if failure_details.get("html_url"):
            embed["fields"].append({
                "name": "🔗 Details",
                "value": f"[View on GitHub]({failure_details['html_url']})",
                "inline": False
            })
        
        payload = {
            "content": f"🚨 **Workflow Failure**: {workflow_name}",
            "embeds": [embed]
        }
        
        try:
            response = requests.post(self.webhook_url, json=payload)
            response.raise_for_status()
            return True
        except requests.exceptions.RequestException as e:
            print(f"Failed to send Discord failure alert: {e}")
            return False
    
    def send_summary_notification(self, report_content: str) -> bool:
        """Send a summary notification with report excerpt"""
        
        # Extract key information from markdown report
        lines = report_content.split('\n')
        summary_lines = []
        
        for line in lines:
            if line.startswith('**Overall Status**:'):
                summary_lines.append(line)
            elif line.startswith('**Success Rate**:'):
                summary_lines.append(line)
            elif line.startswith('**Failed Runs**:'):
                summary_lines.append(line)
            elif line.startswith('**Total Runs**:'):
                summary_lines.append(line)
        
        summary_text = '\n'.join(summary_lines) if summary_lines else "Status report generated"
        
        payload = {
            "content": f"📊 **CI/CD Status Update**\n\n{summary_text}",
        }
        
        try:
            response = requests.post(self.webhook_url, json=payload)
            response.raise_for_status()
            return True
        except requests.exceptions.RequestException as e:
            print(f"Failed to send Discord summary: {e}")
            return False

def main():
    parser = argparse.ArgumentParser(description="Send Discord notifications for CI/CD status")
    parser.add_argument("--webhook", required=True, help="Discord webhook URL")
    parser.add_argument("--status-file", help="Status JSON file")
    parser.add_argument("--report-file", help="Report markdown file")
    parser.add_argument("--workflow-failure", help="Send workflow failure alert")
    parser.add_argument("--failure-details", help="Failure details JSON")
    
    args = parser.parse_args()
    
    notifier = DiscordNotifier(args.webhook)
    
    if args.status_file:
        # Send status notification
        with open(args.status_file, "r") as f:
            status_data = json.load(f)
        
        success = notifier.send_status_notification(status_data)
        if success:
            print("✅ Discord status notification sent successfully")
        else:
            print("❌ Failed to send Discord status notification")
    
    elif args.workflow_failure and args.failure_details:
        # Send workflow failure alert
        with open(args.failure_details, "r") as f:
            failure_data = json.load(f)
        
        success = notifier.send_workflow_failure_alert(args.workflow_failure, failure_data)
        if success:
            print("✅ Discord failure alert sent successfully")
        else:
            print("❌ Failed to send Discord failure alert")
    
    elif args.report_file:
        # Send summary notification
        with open(args.report_file, "r") as f:
            report_content = f.read()
        
        success = notifier.send_summary_notification(report_content)
        if success:
            print("✅ Discord summary notification sent successfully")
        else:
            print("❌ Failed to send Discord summary notification")
    
    else:
        print("❌ No notification type specified")

if __name__ == "__main__":
    main()