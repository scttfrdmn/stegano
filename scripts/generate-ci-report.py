#!/usr/bin/env python3
"""
CI/CD Report Generator

Generates comprehensive reports from CI monitoring data in various formats.
"""

import json
import argparse
from datetime import datetime
from typing import Dict, List, Any
import os

class CIReportGenerator:
    def __init__(self, status_data: Dict):
        self.data = status_data
        self.timestamp = datetime.fromisoformat(status_data["timestamp"].replace("Z", "+00:00"))
    
    def generate_markdown_report(self) -> str:
        """Generate a comprehensive markdown report"""
        report = []
        
        # Header
        report.append(f"# CI/CD Status Report")
        report.append(f"**Repository**: {self.data['repository']}")
        report.append(f"**Generated**: {self.timestamp.strftime('%Y-%m-%d %H:%M:%S UTC')}")
        report.append(f"**Analysis Period**: {self.data['analysis_period_days']} days")
        report.append("")
        
        # Executive Summary
        health = self.data["overall_health"]
        summary = self.data["summary"]
        
        status_emoji = {
            "healthy": "✅",
            "warning": "⚠️", 
            "critical": "🚨"
        }
        
        report.append("## 📊 Executive Summary")
        report.append("")
        report.append(f"**Overall Status**: {status_emoji.get(health['overall_status'], '❓')} {health['overall_status'].title()}")
        report.append(f"**Success Rate**: {health['success_rate']:.1f}%")
        report.append(f"**Total Runs**: {health['total_runs']}")
        report.append(f"**Failed Runs**: {health['failed_runs']}")
        report.append(f"**Average Duration**: {health['avg_duration']/60:.1f} minutes")
        report.append("")
        
        # Key Metrics Table
        report.append("### Key Metrics")
        report.append("")
        report.append("| Metric | Value | Status |")
        report.append("|--------|-------|--------|")
        report.append(f"| Success Rate | {health['success_rate']:.1f}% | {'✅' if health['success_rate'] >= 95 else '⚠️' if health['success_rate'] >= 80 else '🚨'} |")
        report.append(f"| Failed Runs | {health['failed_runs']} | {'✅' if health['failed_runs'] <= 2 else '⚠️' if health['failed_runs'] <= 5 else '🚨'} |")
        report.append(f"| Avg Duration | {health['avg_duration']/60:.1f}m | {'✅' if health['avg_duration'] <= 600 else '⚠️' if health['avg_duration'] <= 1800 else '🚨'} |")
        report.append(f"| Active Issues | {len(health['issues'])} | {'✅' if len(health['issues']) == 0 else '⚠️' if len(health['issues']) <= 3 else '🚨'} |")
        report.append("")
        
        # Issues Section
        if health["issues"]:
            report.append("## ⚠️ Issues Detected")
            report.append("")
            
            severity_order = {"high": 1, "medium": 2, "low": 3}
            sorted_issues = sorted(health["issues"], key=lambda x: severity_order[x["severity"]])
            
            for issue in sorted_issues:
                severity_emoji = {"high": "🚨", "medium": "⚠️", "low": "💡"}
                report.append(f"### {severity_emoji[issue['severity']]} {issue['type'].replace('_', ' ').title()}")
                report.append(f"**Severity**: {issue['severity'].title()}")
                report.append(f"**Description**: {issue['message']}")
                report.append("")
        
        # Failing Workflows
        if self.data["failing_workflows"]:
            report.append("## ❌ Failing Workflows")
            report.append("")
            
            for fw in self.data["failing_workflows"]:
                report.append(f"### {fw['name']}")
                report.append(f"- **Failure Count**: {fw['failure_count']}")
                report.append(f"- **First Failure**: {fw['first_failure']}")
                report.append(f"- **Last Failure**: {fw['last_failure']}")
                
                if fw["recent_failures"]:
                    report.append("- **Recent Failures**:")
                    for failure in fw["recent_failures"][:3]:
                        report.append(f"  - [{failure['branch']}]({failure['html_url']}) - {failure['created_at']}")
                
                report.append("")
        
        # Branch Health
        if self.data["branch_health"]:
            report.append("## 🌿 Branch Health")
            report.append("")
            report.append("| Branch | Runs | Success Rate | Health | Last Success |")
            report.append("|--------|------|--------------|--------|--------------|")
            
            # Sort by health status and success rate
            branches = list(self.data["branch_health"].items())
            health_order = {"critical": 1, "warning": 2, "healthy": 3}
            branches.sort(key=lambda x: (health_order[x[1].get("health", "healthy")], -x[1].get("success_rate", 0)))
            
            for branch, stats in branches:
                health_emoji = {"healthy": "✅", "warning": "⚠️", "critical": "🚨"}
                health_status = stats.get("health", "unknown")
                
                last_success = stats.get("last_success", "Never")
                if last_success != "Never":
                    last_success = datetime.fromisoformat(last_success.replace("Z", "+00:00")).strftime("%Y-%m-%d")
                
                report.append(f"| `{branch}` | {stats['total_runs']} | {stats.get('success_rate', 0):.1f}% | {health_emoji.get(health_status, '❓')} {health_status.title()} | {last_success} |")
            
            report.append("")
        
        # Recent Failures Detail
        if health.get("recent_failures"):
            report.append("## 🔍 Recent Failures Analysis")
            report.append("")
            
            for i, failure in enumerate(health["recent_failures"], 1):
                report.append(f"### {i}. {failure['name']}")
                report.append(f"- **Branch**: {failure['head_branch']}")
                report.append(f"- **Run ID**: {failure['id']}")
                report.append(f"- **Created**: {failure['created_at']}")
                report.append(f"- **View Details**: [GitHub Actions]({failure['html_url']})")
                report.append("")
        
        # Dependencies Status
        deps = self.data.get("dependencies", {})
        if deps:
            report.append("## 📦 Dependencies Status")
            report.append("")
            report.append(f"- **Outdated Dependencies**: {deps.get('outdated_dependencies', 0)}")
            report.append(f"- **Security Vulnerabilities**: {deps.get('security_vulnerabilities', 0)}")
            if deps.get("last_update"):
                report.append(f"- **Last Update**: {deps['last_update']}")
            report.append("")
        
        # Recommendations
        recommendations = self._generate_recommendations()
        if recommendations:
            report.append("## 💡 Recommendations")
            report.append("")
            for rec in recommendations:
                report.append(f"- {rec}")
            report.append("")
        
        # Footer
        report.append("---")
        report.append(f"*Report generated by stegano CI/CD monitoring system*")
        report.append(f"*Next update: {(self.timestamp.replace(minute=0, second=0) + timedelta(hours=1)).strftime('%H:%M UTC')}*")
        
        return "\n".join(report)
    
    def generate_html_report(self) -> str:
        """Generate an HTML dashboard report"""
        health = self.data["overall_health"]
        
        # Determine color scheme based on health
        if health["overall_status"] == "healthy":
            primary_color = "#28a745"  # Green
        elif health["overall_status"] == "warning":
            primary_color = "#ffc107"  # Yellow
        else:
            primary_color = "#dc3545"  # Red
        
        html = f"""
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>CI/CD Status Dashboard - {self.data['repository']}</title>
    <style>
        body {{
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            margin: 0;
            padding: 20px;
            background-color: #f8f9fa;
            color: #333;
        }}
        .container {{
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
            overflow: hidden;
        }}
        .header {{
            background: {primary_color};
            color: white;
            padding: 20px;
            text-align: center;
        }}
        .metrics-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            padding: 20px;
        }}
        .metric-card {{
            background: #f8f9fa;
            border-radius: 8px;
            padding: 20px;
            text-align: center;
            border-left: 4px solid {primary_color};
        }}
        .metric-value {{
            font-size: 2em;
            font-weight: bold;
            color: {primary_color};
        }}
        .metric-label {{
            color: #6c757d;
            margin-top: 5px;
        }}
        .issues-section, .workflows-section {{
            padding: 20px;
            border-top: 1px solid #dee2e6;
        }}
        .issue {{
            background: #fff3cd;
            border: 1px solid #ffeaa7;
            border-radius: 4px;
            padding: 10px;
            margin: 10px 0;
        }}
        .issue.high {{
            background: #f8d7da;
            border-color: #f5c6cb;
        }}
        .issue.medium {{
            background: #fff3cd;
            border-color: #ffeaa7;
        }}
        .issue.low {{
            background: #d1ecf1;
            border-color: #bee5eb;
        }}
        .footer {{
            text-align: center;
            padding: 20px;
            color: #6c757d;
            font-size: 0.9em;
            border-top: 1px solid #dee2e6;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin: 20px 0;
        }}
        th, td {{
            text-align: left;
            padding: 12px;
            border-bottom: 1px solid #dee2e6;
        }}
        th {{
            background-color: #f8f9fa;
            font-weight: 600;
        }}
        .status-badge {{
            padding: 4px 8px;
            border-radius: 12px;
            font-size: 0.8em;
            font-weight: bold;
        }}
        .status-healthy {{ background: #d4edda; color: #155724; }}
        .status-warning {{ background: #fff3cd; color: #856404; }}
        .status-critical {{ background: #f8d7da; color: #721c24; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>CI/CD Status Dashboard</h1>
            <p>{self.data['repository']} • {self.timestamp.strftime('%Y-%m-%d %H:%M:%S UTC')}</p>
        </div>
        
        <div class="metrics-grid">
            <div class="metric-card">
                <div class="metric-value">{health['success_rate']:.1f}%</div>
                <div class="metric-label">Success Rate</div>
            </div>
            <div class="metric-card">
                <div class="metric-value">{health['total_runs']}</div>
                <div class="metric-label">Total Runs</div>
            </div>
            <div class="metric-card">
                <div class="metric-value">{health['failed_runs']}</div>
                <div class="metric-label">Failed Runs</div>
            </div>
            <div class="metric-card">
                <div class="metric-value">{health['avg_duration']/60:.1f}m</div>
                <div class="metric-label">Avg Duration</div>
            </div>
        </div>
"""
        
        # Add issues section if there are issues
        if health.get("issues"):
            html += """
        <div class="issues-section">
            <h2>⚠️ Issues Detected</h2>
"""
            for issue in health["issues"]:
                html += f"""
            <div class="issue {issue['severity']}">
                <strong>{issue['type'].replace('_', ' ').title()}</strong> ({issue['severity'].title()})
                <br>{issue['message']}
            </div>
"""
            html += "        </div>"
        
        # Add branch health table
        if self.data.get("branch_health"):
            html += """
        <div class="workflows-section">
            <h2>🌿 Branch Health</h2>
            <table>
                <thead>
                    <tr>
                        <th>Branch</th>
                        <th>Runs</th>
                        <th>Success Rate</th>
                        <th>Status</th>
                        <th>Last Success</th>
                    </tr>
                </thead>
                <tbody>
"""
            for branch, stats in self.data["branch_health"].items():
                health_status = stats.get("health", "unknown")
                last_success = stats.get("last_success", "Never")
                if last_success != "Never":
                    last_success = datetime.fromisoformat(last_success.replace("Z", "+00:00")).strftime("%Y-%m-%d")
                
                html += f"""
                    <tr>
                        <td><code>{branch}</code></td>
                        <td>{stats['total_runs']}</td>
                        <td>{stats.get('success_rate', 0):.1f}%</td>
                        <td><span class="status-badge status-{health_status}">{health_status.title()}</span></td>
                        <td>{last_success}</td>
                    </tr>
"""
            html += """
                </tbody>
            </table>
        </div>
"""
        
        html += f"""
        <div class="footer">
            Report generated by stegano CI/CD monitoring system<br>
            Next update at {(self.timestamp.replace(minute=0, second=0) + timedelta(hours=1)).strftime('%H:%M UTC')}
        </div>
    </div>
</body>
</html>
"""
        return html
    
    def generate_slack_payload(self) -> Dict:
        """Generate Slack notification payload"""
        health = self.data["overall_health"]
        
        # Determine color
        if health["overall_status"] == "healthy":
            color = "good"
        elif health["overall_status"] == "warning":
            color = "warning"
        else:
            color = "danger"
        
        # Create fields
        fields = [
            {
                "title": "Success Rate",
                "value": f"{health['success_rate']:.1f}%",
                "short": True
            },
            {
                "title": "Failed Runs",
                "value": str(health['failed_runs']),
                "short": True
            },
            {
                "title": "Average Duration",
                "value": f"{health['avg_duration']/60:.1f} minutes",
                "short": True
            },
            {
                "title": "Issues",
                "value": str(len(health['issues'])),
                "short": True
            }
        ]
        
        # Add issue details if any
        text = f"CI/CD Status Report for {self.data['repository']}"
        if health["issues"]:
            text += f"\n\n*Issues found:*"
            for issue in health["issues"][:3]:  # Limit to 3 for brevity
                emoji = {"high": "🚨", "medium": "⚠️", "low": "💡"}
                text += f"\n{emoji[issue['severity']]} {issue['message']}"
        
        return {
            "text": text,
            "attachments": [
                {
                    "color": color,
                    "title": f"Overall Status: {health['overall_status'].title()}",
                    "fields": fields,
                    "footer": "stegano CI/CD Monitor",
                    "ts": int(self.timestamp.timestamp())
                }
            ]
        }
    
    def _generate_recommendations(self) -> List[str]:
        """Generate actionable recommendations based on the current status"""
        recommendations = []
        health = self.data["overall_health"]
        
        if health["success_rate"] < 80:
            recommendations.append("🚨 **Critical**: Success rate is below 80%. Review recent failures and fix blocking issues.")
        
        if health["avg_duration"] > 1800:  # 30 minutes
            recommendations.append("⏱️ **Performance**: Build times are high. Consider optimizing CI workflows or using build caches.")
        
        if health["failed_runs"] > 5:
            recommendations.append("🔧 **Stability**: High number of failed runs detected. Review test reliability and flaky tests.")
        
        # Check for persistent failing workflows
        persistent_failures = [fw for fw in self.data["failing_workflows"] if fw["failure_count"] >= 3]
        if persistent_failures:
            recommendations.append("🔄 **Maintenance**: Some workflows have persistent failures. Consider investigating root causes.")
        
        # Branch-specific recommendations
        problematic_branches = [
            (branch, stats) for branch, stats in self.data["branch_health"].items()
            if stats.get("health") in ["warning", "critical"]
        ]
        if problematic_branches:
            recommendations.append(f"🌿 **Branches**: {len(problematic_branches)} branches need attention. Consider cleaning up or fixing failing branches.")
        
        # Dependencies
        deps = self.data.get("dependencies", {})
        if deps.get("security_vulnerabilities", 0) > 0:
            recommendations.append("🔒 **Security**: Security vulnerabilities detected in dependencies. Update affected packages.")
        
        if deps.get("outdated_dependencies", 0) > 10:
            recommendations.append("📦 **Dependencies**: Many outdated dependencies. Schedule a dependency update sprint.")
        
        return recommendations

def main():
    parser = argparse.ArgumentParser(description="Generate CI/CD status reports")
    parser.add_argument("--input", required=True, help="Input JSON status file")
    parser.add_argument("--output", required=True, help="Output file path")
    parser.add_argument("--format", choices=["markdown", "html", "slack"], default="markdown",
                       help="Output format")
    
    args = parser.parse_args()
    
    # Load status data
    with open(args.input, "r") as f:
        status_data = json.load(f)
    
    # Generate report
    generator = CIReportGenerator(status_data)
    
    if args.format == "markdown":
        content = generator.generate_markdown_report()
    elif args.format == "html":
        content = generator.generate_html_report()
    elif args.format == "slack":
        content = json.dumps(generator.generate_slack_payload(), indent=2)
    
    # Write output
    with open(args.output, "w") as f:
        f.write(content)
    
    print(f"✅ Report generated: {args.output}")

if __name__ == "__main__":
    main()