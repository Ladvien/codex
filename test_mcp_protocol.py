#!/usr/bin/env python3
"""
Test script for MCP search_memory command to isolate potential issues
Tests various edge cases that might cause crashes in Claude Desktop
"""

import json
import sys
import subprocess
import time
import string
import random
from pathlib import Path

def generate_test_queries():
    """Generate various test queries to stress test the search_memory command"""
    return [
        # Basic tests
        {"name": "empty_query", "query": "", "should_fail": True},
        {"name": "simple_query", "query": "test", "limit": 5},
        {"name": "normal_query", "query": "hello world", "limit": 10},
        
        # Edge case limits
        {"name": "min_limit", "query": "test", "limit": 1},
        {"name": "max_limit", "query": "test", "limit": 100},
        {"name": "over_limit", "query": "test", "limit": 101, "should_fail": True},
        {"name": "zero_limit", "query": "test", "limit": 0, "should_fail": True},
        {"name": "negative_limit", "query": "test", "limit": -1, "should_fail": True},
        
        # Threshold tests
        {"name": "min_threshold", "query": "test", "similarity_threshold": 0.0},
        {"name": "max_threshold", "query": "test", "similarity_threshold": 1.0},
        {"name": "over_threshold", "query": "test", "similarity_threshold": 1.1, "should_fail": True},
        {"name": "negative_threshold", "query": "test", "similarity_threshold": -0.1, "should_fail": True},
        
        # Long queries
        {"name": "long_query", "query": "This is a very long query " * 100, "limit": 5},
        {"name": "extremely_long_query", "query": "word " * 10000, "limit": 5},
        
        # Special characters
        {"name": "unicode_query", "query": "héllö wørld 🌍 测试", "limit": 5},
        {"name": "special_chars", "query": "!@#$%^&*()[]{}|;:,.<>?", "limit": 5},
        {"name": "sql_injection", "query": "'; DROP TABLE memories; --", "limit": 5},
        {"name": "json_breaking", "query": '{"malicious": "json"}', "limit": 5},
        {"name": "control_chars", "query": "\x00\x01\x02\x03\x04", "limit": 5},
        
        # Tier tests
        {"name": "working_tier", "query": "test", "tier": "working"},
        {"name": "warm_tier", "query": "test", "tier": "warm"},
        {"name": "cold_tier", "query": "test", "tier": "cold"},
        {"name": "invalid_tier", "query": "test", "tier": "invalid", "should_fail": True},
        
        # Multiple parameters
        {"name": "all_params", "query": "comprehensive test", "limit": 15, "similarity_threshold": 0.7, "tier": "working", "include_metadata": True},
        
        # Stress test with repeated queries
        {"name": "repeated_simple", "query": "test", "limit": 5, "repeat": 10},
    ]

def create_mcp_request(method, params=None, request_id=1):
    """Create a standard MCP JSON-RPC request"""
    request = {
        "jsonrpc": "2.0",
        "id": request_id,
        "method": method
    }
    if params:
        request["params"] = params
    return request

def run_mcp_command(request_data, timeout=30):
    """Run a single MCP command and return the result"""
    try:
        # Convert request to JSON string
        request_json = json.dumps(request_data)
        
        # Run the codex-memory mcp-stdio command
        cmd = ["codex-memory", "mcp-stdio"]
        process = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            cwd=str(Path.home() / "codex")
        )
        
        # Send the request
        stdout, stderr = process.communicate(input=request_json, timeout=timeout)
        
        # Check if there was an MCP error response
        mcp_error = False
        if stdout:
            try:
                response = json.loads(stdout)
                if "error" in response:
                    mcp_error = True
            except json.JSONDecodeError:
                pass
        
        return {
            "success": process.returncode == 0 and not mcp_error,
            "return_code": process.returncode,
            "stdout": stdout,
            "stderr": stderr,
            "timeout": False,
            "mcp_error": mcp_error
        }
        
    except subprocess.TimeoutExpired:
        process.kill()
        return {
            "success": False,
            "return_code": -1,
            "stdout": "",
            "stderr": "Command timed out",
            "timeout": True
        }
    except Exception as e:
        return {
            "success": False,
            "return_code": -1,
            "stdout": "",
            "stderr": str(e),
            "timeout": False
        }

def test_initialization():
    """Test MCP server initialization"""
    print("Testing MCP initialization...")
    
    request = create_mcp_request("initialize", {
        "protocolVersion": "2025-06-18",
        "capabilities": {},
        "clientInfo": {
            "name": "test-client",
            "version": "1.0.0"
        }
    })
    
    result = run_mcp_command(request)
    print(f"  Initialization: {'✓' if result['success'] else '✗'}")
    if not result['success']:
        print(f"    Error: {result['stderr']}")
    return result['success']

def test_tools_list():
    """Test tools/list endpoint"""
    print("Testing tools/list...")
    
    request = create_mcp_request("tools/list")
    result = run_mcp_command(request)
    
    print(f"  Tools list: {'✓' if result['success'] else '✗'}")
    if result['success'] and result['stdout']:
        try:
            response = json.loads(result['stdout'])
            if 'result' in response and 'tools' in response['result']:
                tools = response['result']['tools']
                search_tool = next((t for t in tools if t['name'] == 'search_memory'), None)
                if search_tool:
                    print(f"    Found search_memory tool ✓")
                else:
                    print(f"    Missing search_memory tool ✗")
            else:
                print(f"    Invalid response format ✗")
        except json.JSONDecodeError:
            print(f"    Invalid JSON response ✗")
    else:
        print(f"    Error: {result['stderr']}")
    
    return result['success']

def test_search_memory_queries():
    """Test various search_memory queries"""
    print("Testing search_memory queries...")
    
    test_queries = generate_test_queries()
    results = []
    
    for i, test_case in enumerate(test_queries):
        test_name = test_case["name"]
        should_fail = test_case.get("should_fail", False)
        repeat_count = test_case.get("repeat", 1)
        
        print(f"  Testing {test_name}...", end=" ", flush=True)
        
        # Prepare arguments
        args = {k: v for k, v in test_case.items() if k not in ["name", "should_fail", "repeat"]}
        
        # Run the test (possibly multiple times)
        test_results = []
        for attempt in range(repeat_count):
            request = create_mcp_request("tools/call", {
                "name": "search_memory",
                "arguments": args
            }, request_id=i * 1000 + attempt)
            
            result = run_mcp_command(request, timeout=60)  # Longer timeout for complex queries
            test_results.append(result)
            
            if repeat_count > 1:
                time.sleep(0.1)  # Brief delay between repeated requests
        
        # Analyze results
        success_count = sum(1 for r in test_results if r['success'])
        timeout_count = sum(1 for r in test_results if r['timeout'])
        
        if should_fail:
            # Test should fail
            if success_count == 0:
                print("✓ (correctly failed)")
                status = "pass"
            else:
                print("✗ (should have failed)")
                status = "fail"
        else:
            # Test should succeed
            if success_count == repeat_count:
                print("✓")
                status = "pass"
            elif timeout_count > 0:
                print(f"⚠ ({timeout_count}/{repeat_count} timeouts)")
                status = "timeout"
            else:
                print(f"✗ ({success_count}/{repeat_count} succeeded)")
                status = "fail"
        
        # Record results
        results.append({
            "test_name": test_name,
            "args": args,
            "should_fail": should_fail,
            "repeat_count": repeat_count,
            "success_count": success_count,
            "timeout_count": timeout_count,
            "status": status,
            "results": test_results
        })
        
        # Print error details for failures
        if status in ["fail", "timeout"] and not should_fail:
            for j, result in enumerate(test_results):
                if not result['success']:
                    print(f"    Attempt {j+1}: {result['stderr']}")
                    if result['stdout']:
                        try:
                            response = json.loads(result['stdout'])
                            if 'error' in response:
                                print(f"    MCP Error: {response['error']}")
                        except:
                            print(f"    Raw stdout: {result['stdout'][:200]}...")
    
    return results

def analyze_results(results):
    """Analyze and summarize test results"""
    print("\n" + "="*60)
    print("TEST SUMMARY")
    print("="*60)
    
    total_tests = len(results)
    passed_tests = sum(1 for r in results if r['status'] == 'pass')
    failed_tests = sum(1 for r in results if r['status'] == 'fail')
    timeout_tests = sum(1 for r in results if r['status'] == 'timeout')
    
    print(f"Total tests: {total_tests}")
    print(f"Passed: {passed_tests}")
    print(f"Failed: {failed_tests}")
    print(f"Timeouts: {timeout_tests}")
    print(f"Success rate: {passed_tests/total_tests*100:.1f}%")
    
    # Group by issue type
    validation_failures = []
    timeout_issues = []
    crash_issues = []
    
    for result in results:
        if result['status'] == 'fail':
            if result['should_fail']:
                validation_failures.append(result)
            else:
                crash_issues.append(result)
        elif result['status'] == 'timeout':
            timeout_issues.append(result)
    
    if validation_failures:
        print(f"\nValidation Issues ({len(validation_failures)}):")
        for result in validation_failures:
            print(f"  • {result['test_name']}: Expected failure but succeeded")
    
    if timeout_issues:
        print(f"\nTimeout Issues ({len(timeout_issues)}):")
        for result in timeout_issues:
            print(f"  • {result['test_name']}: {result['timeout_count']}/{result['repeat_count']} timeouts")
    
    if crash_issues:
        print(f"\nCrash/Error Issues ({len(crash_issues)}):")
        for result in crash_issues:
            print(f"  • {result['test_name']}: Unexpected failure")
            # Show first error
            first_error = next((r for r in result['results'] if not r['success']), None)
            if first_error:
                print(f"    Error: {first_error['stderr']}")
    
    # Identify patterns that might cause Claude Desktop crashes
    print(f"\nPotential Claude Desktop Crash Patterns:")
    crash_patterns = []
    
    for result in results:
        if result['status'] in ['fail', 'timeout'] and not result['should_fail']:
            args = result['args']
            
            if 'query' in args:
                query = args['query']
                if len(query) > 1000:
                    crash_patterns.append(f"Long queries (>{len(query)} chars): {result['test_name']}")
                if any(ord(c) < 32 for c in query if c not in '\t\n\r'):
                    crash_patterns.append(f"Control characters: {result['test_name']}")
                if '"' in query or '{' in query:
                    crash_patterns.append(f"JSON-breaking chars: {result['test_name']}")
            
            if result['timeout_count'] > 0:
                crash_patterns.append(f"Timeout pattern: {result['test_name']}")
    
    if crash_patterns:
        for pattern in set(crash_patterns):  # Remove duplicates
            print(f"  • {pattern}")
    else:
        print("  None detected - MCP protocol appears robust")
    
    return {
        'total': total_tests,
        'passed': passed_tests,
        'failed': failed_tests,
        'timeouts': timeout_tests,
        'crash_patterns': crash_patterns
    }

def write_results_to_file(results, summary):
    """Write detailed results to team_chat.md"""
    team_chat_path = Path.home() / "codex" / "team_chat.md"
    
    # Read existing content
    if team_chat_path.exists():
        with open(team_chat_path, 'r') as f:
            content = f.read()
    else:
        content = "# Team Chat\n\n"
    
    # Add our test results
    timestamp = time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime())
    
    test_section = f"""
## MCP Protocol Testing ({timestamp})

### Test Summary
- **Total tests**: {summary['total']}
- **Passed**: {summary['passed']}
- **Failed**: {summary['failed']}
- **Timeouts**: {summary['timeouts']}
- **Success rate**: {summary['passed']/summary['total']*100:.1f}%

### Key Findings

#### Potential Crash Patterns
"""
    
    if summary['crash_patterns']:
        for pattern in set(summary['crash_patterns']):
            test_section += f"- {pattern}\n"
    else:
        test_section += "- No crash patterns detected - MCP protocol appears robust\n"
    
    test_section += f"""
#### Timeout Issues
"""
    
    timeout_results = [r for r in results if r['status'] == 'timeout']
    if timeout_results:
        for result in timeout_results:
            test_section += f"- **{result['test_name']}**: {result['timeout_count']}/{result['repeat_count']} requests timed out\n"
    else:
        test_section += "- No timeout issues detected\n"
    
    test_section += f"""
#### Validation Issues
"""
    
    validation_failures = [r for r in results if r['status'] == 'fail' and r['should_fail']]
    if validation_failures:
        for result in validation_failures:
            test_section += f"- **{result['test_name']}**: Expected to fail but succeeded\n"
    else:
        test_section += "- All validation tests behaved as expected\n"
    
    test_section += f"""
### Detailed Results

| Test Name | Status | Query Length | Special Features | Result |
|-----------|---------|--------------|------------------|---------|
"""
    
    for result in results:
        query = result['args'].get('query', '')
        query_len = len(query)
        
        features = []
        if query_len > 100:
            features.append("long")
        if any(ord(c) < 32 for c in query if c not in '\t\n\r'):
            features.append("control-chars")
        if any(ord(c) > 127 for c in query):
            features.append("unicode")
        if result.get('repeat_count', 1) > 1:
            features.append("repeated")
        
        features_str = ", ".join(features) if features else "normal"
        
        status_emoji = {
            'pass': '✅',
            'fail': '❌',
            'timeout': '⏱️'
        }.get(result['status'], '❓')
        
        test_section += f"| {result['test_name']} | {status_emoji} | {query_len} | {features_str} | {result['status']} |\n"
    
    # Analyze patterns for recommendations
    long_query_timeouts = any(len(r['args'].get('query', '')) > 1000 and r['status'] == 'timeout' for r in results)
    
    # Check for control character issues (separate variable to avoid f-string backslash issue)
    whitespace_chars = '\t\n\r'
    control_char_issues = any(
        any(ord(c) < 32 for c in r['args'].get('query', '') if c not in whitespace_chars) 
        and r['status'] != 'pass' for r in results
    )
    
    validation_working = all(r['status'] == 'pass' for r in results if r['should_fail'])
    
    test_section += f"""
### Recommendations

Based on these tests, the following recommendations emerge:

1. **Query Length**: {"Long queries (>1000 chars) may cause timeouts" if long_query_timeouts else "Query length handling appears robust"}

2. **Special Characters**: {"Control characters may cause issues" if control_char_issues else "Special character handling appears robust"}

3. **Parameter Validation**: {"Parameter validation working correctly" if validation_working else "Some parameter validation issues detected"}

4. **Performance**: {"Some performance issues detected with timeouts" if summary['timeouts'] > 0 else "Performance appears adequate"}

"""
    
    # Insert the test section
    if "## MCP Protocol Testing" in content:
        # Replace existing section
        lines = content.split('\n')
        start_idx = None
        end_idx = None
        
        for i, line in enumerate(lines):
            if line.startswith("## MCP Protocol Testing"):
                start_idx = i
            elif start_idx is not None and line.startswith("## ") and not line.startswith("## MCP Protocol Testing"):
                end_idx = i
                break
        
        if start_idx is not None:
            if end_idx is not None:
                # Replace existing section
                lines = lines[:start_idx] + test_section.strip().split('\n') + lines[end_idx:]
            else:
                # Replace to end of file
                lines = lines[:start_idx] + test_section.strip().split('\n')
            
            content = '\n'.join(lines)
        else:
            # Append new section
            content += test_section
    else:
        # Append new section
        content += test_section
    
    # Write back to file
    with open(team_chat_path, 'w') as f:
        f.write(content)
    
    print(f"\nDetailed results written to: {team_chat_path}")

def main():
    """Main test function"""
    print("MCP Protocol Test Suite for search_memory")
    print("="*60)
    
    # Test basic MCP functionality first
    if not test_initialization():
        print("❌ MCP initialization failed - cannot continue")
        return 1
    
    if not test_tools_list():
        print("❌ Tools list failed - cannot continue")
        return 1
    
    # Test search_memory with various parameters
    results = test_search_memory_queries()
    summary = analyze_results(results)
    
    # Write results to team chat
    write_results_to_file(results, summary)
    
    # Return appropriate exit code
    if summary['failed'] > 0 or summary['timeouts'] > 0:
        return 1
    else:
        return 0

if __name__ == "__main__":
    sys.exit(main())