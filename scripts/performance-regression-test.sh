#!/bin/bash

# Production Performance Regression Test Script
# Story 10: Performance optimization validation

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BASELINE_FILE="${PROJECT_ROOT}/benchmarks/baseline_performance.json"
CURRENT_FILE="${PROJECT_ROOT}/benchmarks/current_performance.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🚀 Production Performance Regression Test"
echo "========================================"

# Create benchmarks directory if it doesn't exist
mkdir -p "${PROJECT_ROOT}/benchmarks"

# Function to run performance tests and capture metrics
run_performance_tests() {
    echo "📊 Running performance tests..."
    
    # Run comprehensive performance tests
    cargo test --release performance_tests::test_comprehensive_performance_suite \
        -- --nocapture 2>&1 | tee /tmp/perf_output.txt
    
    # Run load tests
    cargo test --release e2e_performance_load::test_concurrent_operations_load \
        -- --nocapture 2>&1 | tee -a /tmp/perf_output.txt
    
    # Extract key metrics and create JSON
    python3 << 'EOF'
import json
import re
import sys
import os

# Read performance output
with open('/tmp/perf_output.txt', 'r') as f:
    output = f.read()

# Extract metrics using regex patterns
metrics = {}

# P95 latency patterns
p95_pattern = r'P95:\s*(\d+\.?\d*)\s*ms'
p95_matches = re.findall(p95_pattern, output)
if p95_matches:
    metrics['p95_latency_ms'] = float(max(p95_matches))

# Operations per second patterns
ops_pattern = r'(\d+\.?\d*)\s*ops/sec'
ops_matches = re.findall(ops_pattern, output)
if ops_matches:
    metrics['max_ops_per_second'] = float(max(ops_matches))

# Memory headroom pattern
headroom_pattern = r'memory.*headroom.*(\d+\.?\d*)%'
headroom_matches = re.findall(headroom_pattern, output, re.IGNORECASE)
if headroom_matches:
    metrics['memory_headroom_percent'] = float(max(headroom_matches))

# Batch operation throughput
batch_pattern = r'Batch.*(\d+).*(\d+\.?\d*)\s*ops/sec'
batch_matches = re.findall(batch_pattern, output)
if batch_matches:
    metrics['batch_throughput'] = max(float(match[1]) for match in batch_matches)

# Token reduction (if context reduction implemented)
token_pattern = r'token.*reduction.*(\d+\.?\d*)%'
token_matches = re.findall(token_pattern, output, re.IGNORECASE)
if token_matches:
    metrics['token_reduction_percent'] = float(max(token_matches))

# Add timestamp
import datetime
metrics['timestamp'] = datetime.datetime.now().isoformat()
metrics['test_run_id'] = f"perf_{int(datetime.datetime.now().timestamp())}"

# Write current performance metrics
output_file = sys.argv[1] if len(sys.argv) > 1 else 'current_performance.json'
with open(output_file, 'w') as f:
    json.dump(metrics, f, indent=2)

print(f"📋 Performance metrics saved to {output_file}")
print(json.dumps(metrics, indent=2))
EOF
    
    # Move the generated file to the correct location
    if [ -f current_performance.json ]; then
        mv current_performance.json "$CURRENT_FILE"
    fi
}

# Function to compare against baseline
compare_with_baseline() {
    if [ ! -f "$BASELINE_FILE" ]; then
        echo "📝 No baseline found. Creating baseline from current run..."
        cp "$CURRENT_FILE" "$BASELINE_FILE"
        echo -e "${GREEN}✅ Baseline created successfully${NC}"
        return 0
    fi
    
    echo "🔍 Comparing with baseline..."
    
    python3 << 'EOF'
import json
import sys
import os

def load_json_safe(filepath):
    try:
        with open(filepath, 'r') as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError) as e:
        print(f"Error loading {filepath}: {e}")
        return {}

baseline_file = sys.argv[1]
current_file = sys.argv[2]

baseline = load_json_safe(baseline_file)
current = load_json_safe(current_file)

if not baseline or not current:
    print("❌ Could not load performance data for comparison")
    sys.exit(1)

# Performance thresholds (Story 10 requirements)
thresholds = {
    'p95_latency_ms': {'max': 2000, 'regression_tolerance': 0.1},  # <2s, 10% tolerance
    'memory_headroom_percent': {'min': 20, 'regression_tolerance': 0.05},  # >20%, 5% tolerance
    'max_ops_per_second': {'regression_tolerance': 0.15},  # 15% regression tolerance
    'batch_throughput': {'regression_tolerance': 0.15},    # 15% regression tolerance
    'token_reduction_percent': {'min': 90, 'regression_tolerance': 0.05},  # >90%, 5% tolerance
}

print("Performance Comparison Report")
print("=" * 50)

regressions = []
improvements = []
requirements_met = True

for metric, current_value in current.items():
    if metric in ['timestamp', 'test_run_id']:
        continue
        
    baseline_value = baseline.get(metric)
    if baseline_value is None:
        continue
        
    if not isinstance(current_value, (int, float)) or not isinstance(baseline_value, (int, float)):
        continue
    
    threshold_config = thresholds.get(metric, {})
    
    # Calculate percentage change
    if baseline_value != 0:
        pct_change = (current_value - baseline_value) / baseline_value
    else:
        pct_change = 0
    
    # Check absolute requirements
    if 'max' in threshold_config and current_value > threshold_config['max']:
        print(f"❌ {metric}: {current_value:.2f} exceeds maximum {threshold_config['max']}")
        requirements_met = False
    
    if 'min' in threshold_config and current_value < threshold_config['min']:
        print(f"❌ {metric}: {current_value:.2f} below minimum {threshold_config['min']}")
        requirements_met = False
    
    # Check for regression
    tolerance = threshold_config.get('regression_tolerance', 0.1)
    
    if metric in ['p95_latency_ms']:  # Lower is better
        if pct_change > tolerance:
            regressions.append(f"{metric}: +{pct_change*100:.1f}% slower ({baseline_value:.2f} -> {current_value:.2f})")
        elif pct_change < -0.05:  # Improvement threshold
            improvements.append(f"{metric}: {-pct_change*100:.1f}% faster ({baseline_value:.2f} -> {current_value:.2f})")
    else:  # Higher is better
        if pct_change < -tolerance:
            regressions.append(f"{metric}: {-pct_change*100:.1f}% worse ({baseline_value:.2f} -> {current_value:.2f})")
        elif pct_change > 0.05:  # Improvement threshold
            improvements.append(f"{metric}: +{pct_change*100:.1f}% better ({baseline_value:.2f} -> {current_value:.2f})")
    
    # Display current vs baseline
    change_indicator = "📈" if pct_change > 0 else "📉" if pct_change < 0 else "➡️"
    print(f"{change_indicator} {metric}: {current_value:.2f} (baseline: {baseline_value:.2f}, {pct_change*100:+.1f}%)")

print("\n" + "=" * 50)

if regressions:
    print("⚠️  Performance Regressions Detected:")
    for regression in regressions:
        print(f"   • {regression}")
    requirements_met = False

if improvements:
    print("✅ Performance Improvements:")
    for improvement in improvements:
        print(f"   • {improvement}")

print(f"\n📊 Story 10 Requirements Status:")
print(f"   • P95 latency < 2s: {'✅' if current.get('p95_latency_ms', 0) < 2000 else '❌'}")
print(f"   • Memory headroom ≥ 20%: {'✅' if current.get('memory_headroom_percent', 0) >= 20 else '❌'}")
print(f"   • Token reduction ≥ 90%: {'✅' if current.get('token_reduction_percent', 0) >= 90 else '❌'}")

if requirements_met and not regressions:
    print(f"\n🎉 All performance requirements met!")
    sys.exit(0)
else:
    print(f"\n❌ Performance requirements not met or regressions detected")
    sys.exit(1)
EOF
    
    return $?
}

# Main execution
main() {
    local compare_against_baseline=false
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --compare-against)
                if [ "$2" = "baseline" ]; then
                    compare_against_baseline=true
                fi
                shift 2
                ;;
            --help)
                echo "Usage: $0 [--compare-against baseline]"
                echo "  --compare-against baseline: Compare current performance with baseline"
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    # Run performance tests
    run_performance_tests
    
    # Compare with baseline if requested
    if [ "$compare_against_baseline" = true ]; then
        compare_with_baseline "$BASELINE_FILE" "$CURRENT_FILE"
        comparison_result=$?
        
        if [ $comparison_result -eq 0 ]; then
            echo -e "\n${GREEN}✅ Performance regression test passed!${NC}"
        else
            echo -e "\n${RED}❌ Performance regression test failed!${NC}"
            exit 1
        fi
    else
        echo -e "\n${YELLOW}ℹ️  Performance test completed. Use --compare-against baseline to check for regressions.${NC}"
    fi
}

main "$@"