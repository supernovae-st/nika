# DevOps Automation Recipes

Production-ready workflows for deployment verification, log analysis, monitoring, infrastructure auditing, and CI/CD integration using Nika's `exec:`, `fetch:`, and `agent:` verbs.

---

## Recipe 1: Deployment Health Check Pipeline

**Problem:** After deploying a new version, you need to verify that all services are healthy, endpoints are responsive, and no regressions were introduced.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: deployment-health-check
description: "Post-deployment verification: health checks, response validation, smoke tests"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  environment: "staging"
  version: "1.2.3"
  base_url: "https://httpbin.org"

artifacts:
  dir: ./output/deployment-check

tasks:
  # Smoke test: check all critical endpoints
  - id: endpoint_checks
    for_each:
      - { path: "/get", name: "GET API", expected_status: 200 }
      - { path: "/status/200", name: "Health Check", expected_status: 200 }
      - { path: "/headers", name: "Headers", expected_status: 200 }
      - { path: "/ip", name: "IP Info", expected_status: 200 }
      - { path: "/user-agent", name: "User Agent", expected_status: 200 }
    as: endpoint
    concurrency: 5
    fail_fast: false
    fetch:
      url: "{{inputs.base_url}}{{with.endpoint.path}}"
      response: full
      timeout: 10
    retry:
      max_attempts: 3
      delay_ms: 2000
      backoff: 2.0

  # Check response times with a latency endpoint
  - id: latency_check
    fetch:
      url: "{{inputs.base_url}}/delay/1"
      response: full
      timeout: 15
    retry:
      max_attempts: 2
      delay_ms: 1000

  # Verify redirect handling
  - id: redirect_check
    fetch:
      url: "{{inputs.base_url}}/redirect/2"
      response: full
      timeout: 15

  # Check security headers
  - id: security_headers
    fetch:
      url: "{{inputs.base_url}}"
      extract: metadata
      timeout: 10

  # Analyze deployment health
  - id: health_report
    depends_on: [endpoint_checks, latency_check, redirect_check, security_headers]
    with:
      endpoints: $endpoint_checks
      latency: $latency_check
      redirects: $redirect_check
      security: $security_headers
      version: $inputs.version
      environment: $inputs.environment
    infer:
      system: "You are a site reliability engineer validating a deployment."
      prompt: |
        Deployment: v{{inputs.version}} to {{inputs.environment}}

        Endpoint Checks (full response envelopes):
        {{with.endpoints | first(3000)}}

        Latency Check:
        {{with.latency | first(500)}}

        Redirect Handling:
        {{with.redirects | first(500)}}

        Security Metadata:
        {{with.security | first(500)}}

        Generate a deployment health report:
        1. Endpoint Status Summary (all passed? which failed?)
        2. Response Time Assessment (acceptable latency?)
        3. Redirect Chain Verification
        4. Security Header Audit (CSP, HSTS, X-Frame-Options)
        5. DEPLOYMENT VERDICT: PASS or FAIL with reasons
        6. Rollback recommendation if FAIL
      temperature: 0.1
      max_tokens: 2000
    structured:
      schema:
        type: object
        properties:
          version:
            type: string
          environment:
            type: string
          verdict:
            type: string
            enum: ["PASS", "FAIL", "WARNING"]
          endpoints_passed:
            type: integer
          endpoints_total:
            type: integer
          avg_response_ms:
            type: integer
          critical_issues:
            type: array
            items:
              type: string
          recommendations:
            type: array
            items:
              type: string
        required: [version, environment, verdict, endpoints_passed, endpoints_total]
    artifact:
      path: health-report.json
      format: json

  # Log deployment check to history
  - id: log_check
    depends_on: [health_report]
    with:
      report: $health_report
    exec:
      command: |
        echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] v{{inputs.version}} -> {{inputs.environment}}: {{with.report}}" | head -c 300
      shell: true
    artifact:
      path: deployment-history.log
      mode: append
```

**Explanation:**

The `response: full` mode on endpoint checks returns the complete HTTP response envelope (status code, headers, body, final URL), enabling the LLM to analyze response codes and security headers. The `retry:` with exponential backoff handles transient failures during deployment. The `fail_fast: false` ensures all endpoints are checked even if some fail. The `mode: append` on the history log creates a persistent deployment record.

**Expected Output:** A structured health report JSON with pass/fail verdict and an appending deployment history log.

---

## Recipe 2: Log Analysis and Anomaly Detection

**Problem:** You need to analyze application logs, detect anomalies, and generate alerts with actionable recommendations.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: log-analyzer
description: "Analyze logs for anomalies, patterns, and generate alerts"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/log-analysis

tasks:
  # Simulate log collection (in production, use exec: with real log commands)
  - id: collect_logs
    exec:
      command: |
        echo '[
          {"timestamp": "2026-03-23T10:00:01Z", "level": "INFO", "service": "api", "message": "Request processed", "duration_ms": 45},
          {"timestamp": "2026-03-23T10:00:02Z", "level": "WARN", "service": "api", "message": "Slow query detected", "duration_ms": 2500},
          {"timestamp": "2026-03-23T10:00:03Z", "level": "ERROR", "service": "auth", "message": "Token validation failed", "user_id": "usr_123"},
          {"timestamp": "2026-03-23T10:00:04Z", "level": "INFO", "service": "api", "message": "Request processed", "duration_ms": 38},
          {"timestamp": "2026-03-23T10:00:05Z", "level": "ERROR", "service": "auth", "message": "Token validation failed", "user_id": "usr_456"},
          {"timestamp": "2026-03-23T10:00:06Z", "level": "CRITICAL", "service": "db", "message": "Connection pool exhausted", "active_connections": 100},
          {"timestamp": "2026-03-23T10:00:07Z", "level": "WARN", "service": "api", "message": "Rate limit approaching", "current_rate": 950, "limit": 1000},
          {"timestamp": "2026-03-23T10:00:08Z", "level": "INFO", "service": "api", "message": "Request processed", "duration_ms": 1200},
          {"timestamp": "2026-03-23T10:00:09Z", "level": "ERROR", "service": "auth", "message": "Token validation failed", "user_id": "usr_789"},
          {"timestamp": "2026-03-23T10:00:10Z", "level": "ERROR", "service": "db", "message": "Query timeout after 30000ms", "query": "SELECT * FROM users"}
        ]'
      shell: true
    artifact:
      path: raw-logs.json
      format: json

  # Analyze by service
  - id: service_analysis
    depends_on: [collect_logs]
    with:
      logs: $collect_logs
    for_each:
      - { service: "api", focus: "response times and rate limits" }
      - { service: "auth", focus: "authentication failures and patterns" }
      - { service: "db", focus: "database performance and connection issues" }
    as: svc
    concurrency: 3
    infer:
      prompt: |
        Analyze logs for the {{with.svc.service}} service:
        {{with.logs}}

        Focus: {{with.svc.focus}}

        Provide:
        1. Error count and types
        2. Performance metrics
        3. Anomaly detection
        4. Root cause hypothesis
      temperature: 0.2
      max_tokens: 800

  # Generate alert recommendations
  - id: alerts
    depends_on: [service_analysis, collect_logs]
    with:
      analysis: $service_analysis
      raw_logs: $collect_logs
    infer:
      prompt: |
        Based on this log analysis, generate alert recommendations:

        Service Analysis:
        {{with.analysis}}

        Raw Logs:
        {{with.raw_logs | first(2000)}}

        Generate:
        1. CRITICAL alerts (immediate action required)
        2. WARNING alerts (investigate soon)
        3. INFO alerts (monitor and track)
        4. Suggested alert rules (metric + threshold + action)
        5. Escalation procedures
      response_format: json
      temperature: 0.1
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          alerts:
            type: array
            items:
              type: object
              properties:
                severity:
                  type: string
                  enum: ["critical", "warning", "info"]
                service:
                  type: string
                message:
                  type: string
                action:
                  type: string
              required: [severity, service, message, action]
          escalation_needed:
            type: boolean
          root_causes:
            type: array
            items:
              type: string
        required: [alerts, escalation_needed]
    artifact:
      path: alert-report.json
      format: json
```

**Explanation:**

The `for_each:` block analyzes logs by service in parallel, letting the LLM focus on each service's specific concerns. The structured alert output ensures consistent severity classification and actionable messages. In production, the `collect_logs` task would use `exec:` commands like `journalctl`, `kubectl logs`, or API calls to log aggregation services.

**Expected Output:** Raw log archive and a structured alert report with severity-classified alerts.

---

## Recipe 3: Infrastructure Audit Agent

**Problem:** You need an autonomous agent that can explore system configuration, check for security issues, and produce an audit report.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: infrastructure-audit
description: "Autonomous infrastructure audit with file exploration and reporting"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/infra-audit

tasks:
  # Gather system information
  - id: system_info
    exec:
      command: |
        echo '{
          "os": "'$(uname -s)'",
          "arch": "'$(uname -m)'",
          "hostname": "'$(hostname)'",
          "uptime": "'$(uptime | tr -s ' ')'",
          "disk_usage": "'$(df -h / | tail -1 | tr -s ' ')'",
          "memory": "'$(vm_stat 2>/dev/null | head -5 | tr '\n' '; ' || echo "N/A")'"
        }'
      shell: true
      timeout: 10

  # Check network connectivity
  - id: network_check
    for_each:
      - { host: "https://httpbin.org/get", name: "External API" }
      - { host: "https://github.com", name: "GitHub" }
    as: target
    concurrency: 2
    fail_fast: false
    fetch:
      url: "{{with.target.host}}"
      response: full
      timeout: 10

  # Audit agent explores configuration
  - id: config_audit
    depends_on: [system_info]
    with:
      system: $system_info
    agent:
      system: |
        You are an infrastructure security auditor.
        System info: {{with.system}}

        Audit checklist:
        1. Use nika_glob to find configuration files
        2. Use nika_grep to search for security issues (passwords, keys, insecure settings)
        3. Use nika_read to examine critical config files
        4. Use nika_log to track findings
        5. Call nika_complete with the audit report

        Focus: security, compliance, best practices.
      prompt: |
        Perform an infrastructure security audit.
        Check for:
        - Exposed credentials
        - Insecure configurations
        - Missing security headers
        - Outdated dependencies
        - File permission issues

        Call nika_complete with your findings.
      tools:
        - "nika:glob"
        - "nika:read"
        - "nika:grep"
        - "nika:log"
      max_turns: 8
      max_tokens: 2000
      token_budget: 20000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 200
          on_failure: retry
        - type: regex
          pattern: "(?i)(finding|risk|recommendation)"
          message: "Audit must include findings and recommendations"
          on_failure: retry
      limits:
        max_turns: 8
        max_tokens: 40000
        max_cost_usd: 1.50
        max_duration_secs: 180
    artifact:
      path: audit-findings.md

  # Generate structured compliance report
  - id: compliance_report
    depends_on: [config_audit, network_check, system_info]
    with:
      audit: $config_audit
      network: $network_check
      system: $system_info
    infer:
      prompt: |
        Create a compliance report from:

        Security Audit: {{with.audit | first(2000)}}
        Network Checks: {{with.network | first(1000)}}
        System Info: {{with.system}}

        Format as a compliance checklist with pass/fail for each category.
      response_format: json
      temperature: 0.1
      max_tokens: 1500
    structured:
      schema:
        type: object
        properties:
          overall_score:
            type: integer
          risk_level:
            type: string
            enum: ["low", "medium", "high", "critical"]
          categories:
            type: array
            items:
              type: object
              properties:
                name:
                  type: string
                status:
                  type: string
                  enum: ["pass", "fail", "warning"]
                findings:
                  type: array
                  items:
                    type: string
              required: [name, status]
        required: [overall_score, risk_level, categories]
    artifact:
      path: compliance-report.json
      format: json
```

**Explanation:**

The audit agent uses `nika:grep` to search for security anti-patterns across configuration files. The `exec:` task gathers system information using standard Unix commands. The `fail_fast: false` on network checks ensures that connectivity issues on one target do not prevent checking others.

**Expected Output:** Audit findings markdown and a structured compliance report with pass/fail categories.

---

## Recipe 4: CI/CD Pipeline Validator

**Problem:** You need to validate that a CI/CD pipeline configuration is correct before pushing to production.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: cicd-validator
description: "Validate CI/CD pipeline configuration and generate deployment plan"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  target_branch: "main"
  deploy_env: "production"

artifacts:
  dir: ./output/cicd-validation

tasks:
  # Check git status
  - id: git_status
    exec:
      command: "git status --porcelain 2>/dev/null || echo 'Not a git repo'"
      shell: true
      timeout: 5

  # Check for uncommitted changes
  - id: git_log
    exec:
      command: "git log --oneline -10 2>/dev/null || echo 'No git history'"
      shell: true
      timeout: 5

  # Run tests (simulated)
  - id: run_tests
    exec:
      command: "echo '{\"tests_run\": 156, \"passed\": 154, \"failed\": 2, \"skipped\": 3}'"
      shell: true
      timeout: 60

  # Check dependencies
  - id: check_deps
    exec:
      command: "echo '{\"outdated\": 3, \"vulnerable\": 1, \"total\": 45}'"
      shell: true
      timeout: 30

  # Search for deployment blockers
  - id: scan_blockers
    agent:
      system: |
        You are a CI/CD pipeline validator.
        Search for deployment blockers:
        1. Use nika_grep to find TODO, FIXME, HACK in source code
        2. Use nika_glob to check for required files (Dockerfile, CI config)
        3. Use nika_read to verify critical configuration
        4. Call nika_complete with blocker report
      prompt: |
        Scan for deployment blockers before deploying to {{inputs.deploy_env}}.
        Target branch: {{inputs.target_branch}}
        Call nika_complete with your findings.
      tools:
        - "nika:glob"
        - "nika:read"
        - "nika:grep"
        - "nika:log"
      max_turns: 6
      max_tokens: 1500
      token_budget: 12000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 100
          on_failure: retry

  # Generate deployment decision
  - id: deployment_decision
    depends_on: [git_status, git_log, run_tests, check_deps, scan_blockers]
    with:
      git: $git_status
      log: $git_log
      tests: $run_tests
      deps: $check_deps
      blockers: $scan_blockers
    infer:
      prompt: |
        Make a deployment decision for {{inputs.deploy_env}}:

        Git Status: {{with.git}}
        Recent Commits: {{with.log}}
        Test Results: {{with.tests}}
        Dependencies: {{with.deps}}
        Blocker Scan: {{with.blockers | first(1500)}}

        Decision criteria:
        - All tests must pass (0 failures)
        - No critical vulnerabilities
        - No uncommitted changes
        - No TODO/FIXME in critical paths

        Return: deploy (yes/no), confidence, blockers list, pre-deploy actions.
      response_format: json
      temperature: 0.1
      max_tokens: 1000
    structured:
      schema:
        type: object
        properties:
          deploy:
            type: boolean
          confidence:
            type: integer
          environment:
            type: string
          blockers:
            type: array
            items:
              type: string
          pre_deploy_actions:
            type: array
            items:
              type: string
          post_deploy_checks:
            type: array
            items:
              type: string
        required: [deploy, confidence, environment, blockers]
    artifact:
      path: deployment-decision.json
      format: json
```

**Explanation:**

This workflow combines `exec:` for gathering build/test data with an agent for code scanning and `infer:` for the final deployment decision. The `structured:` output ensures a machine-readable go/no-go decision with specific blockers and required actions. This can be integrated into a CI/CD pipeline as a pre-deployment gate.

**Expected Output:** A structured deployment decision JSON with pass/fail, confidence score, and pre-deploy actions.

---

## Recipe 5: Monitoring Dashboard Generator

**Problem:** You need to generate monitoring charts and reports from system metrics on a regular basis.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: monitoring-dashboard
description: "Generate monitoring charts and reports from system metrics"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/monitoring

tasks:
  # Collect metrics
  - id: collect_metrics
    exec:
      command: |
        echo '{
          "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'",
          "cpu_pct": [65, 72, 58, 81, 45],
          "mem_pct": [45, 52, 48, 55, 42],
          "disk_pct": [67, 68, 69, 70, 71],
          "requests_per_sec": [1200, 1450, 980, 1650, 1100],
          "error_rate": [0.02, 0.01, 0.05, 0.01, 0.03],
          "labels": ["10:00", "10:15", "10:30", "10:45", "11:00"]
        }'
      shell: true

  # Generate resource chart
  - id: resource_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "line"
        title: "System Resources (15-min intervals)"
        width: 900
        height: 500
        series:
          - name: "CPU %"
            data: [65, 72, 58, 81, 45]
          - name: "Memory %"
            data: [45, 52, 48, 55, 42]
          - name: "Disk %"
            data: [67, 68, 69, 70, 71]
        labels: ["10:00", "10:15", "10:30", "10:45", "11:00"]
    artifact:
      path: resources.png
      format: binary

  # Generate traffic chart
  - id: traffic_chart
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "Request Rate and Error Rate"
        width: 900
        height: 500
        series:
          - name: "Requests/sec"
            data: [1200, 1450, 980, 1650, 1100]
        labels: ["10:00", "10:15", "10:30", "10:45", "11:00"]
    artifact:
      path: traffic.png
      format: binary

  # Visual analysis with charts
  - id: analysis
    depends_on: [collect_metrics, resource_chart, traffic_chart]
    with:
      metrics: $collect_metrics
      resources: $resource_chart
      traffic: $traffic_chart
    infer:
      content:
        - type: image
          source: "{{with.resources.media[0].hash}}"
          detail: high
        - type: image
          source: "{{with.traffic.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Analyze these monitoring charts with the raw metrics:
            {{with.metrics}}

            Provide:
            1. Current system health status
            2. Resource utilization trends
            3. Traffic pattern analysis
            4. Anomalies detected
            5. Capacity planning recommendations
            6. Alerting threshold suggestions
      temperature: 0.2
      max_tokens: 2000
    artifact:
      path: monitoring-report.md
```

**Explanation:**

The `nika:chart` tool generates PNG charts from raw data. Two charts are generated in parallel (no `depends_on:` between them), then the vision-capable LLM analyzes both charts alongside raw metrics. This produces a report that combines visual pattern recognition with data-driven analysis.

**Expected Output:** Resource and traffic chart PNGs plus a comprehensive monitoring report.

---

## Key Patterns for DevOps Automation

### System Commands

```yaml
exec:
  command: "kubectl get pods -o json"
  shell: true
  timeout: 30
```

### Health Check Pattern

```yaml
fetch:
  url: "https://api.example.com/health"
  response: full        # Get status code + headers
  timeout: 10
retry:
  max_attempts: 3
  delay_ms: 2000
  backoff: 2.0
```

### Deployment History

```yaml
artifact:
  path: deploy-history.log
  mode: append          # Append to existing log
```

### Parallel Endpoint Checks

```yaml
for_each:
  - { url: "/api/health", name: "API" }
  - { url: "/db/health", name: "Database" }
as: endpoint
concurrency: 5
fail_fast: false        # Check all even if some fail
```
