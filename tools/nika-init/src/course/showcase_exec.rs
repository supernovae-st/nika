// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Showcase workflows — 20 real, runnable workflows using exec: and fetch: only
//!
//! No LLM provider or API key required. Demonstrates practical automation
//! patterns with shell commands and public HTTP APIs. Organized in 5 categories:
//! system (4), devops (4), network (4), api (4), data (4).

use super::showcase::ShowcaseWorkflow;

/// All 20 exec/fetch showcase workflows — zero LLM dependency
pub static SHOWCASE_EXEC: &[ShowcaseWorkflow] = &[
    // ═════════════════════════════════════════════════════════════════════════
    // CATEGORY: SYSTEM (1-4)
    // ═════════════════════════════════════════════════════════════════════════
    ShowcaseWorkflow {
        name: "system-health-dashboard",
        description: "Collect hostname, uptime, disk, and memory into a formatted report",
        category: "system",
        content: SYSTEM_HEALTH_DASHBOARD,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "environment-reporter",
        description: "Audit environment variables grouped by category",
        category: "system",
        content: ENVIRONMENT_REPORTER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "filesystem-analyzer",
        description: "Project statistics: file counts, sizes, line counts",
        category: "system",
        content: FILESYSTEM_ANALYZER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "homebrew-audit",
        description: "Check outdated Homebrew packages and cask status",
        category: "system",
        content: HOMEBREW_AUDIT,
        requires_llm: false,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // CATEGORY: DEVOPS (5-8)
    // ═════════════════════════════════════════════════════════════════════════
    ShowcaseWorkflow {
        name: "git-changelog",
        description: "Generate a formatted Markdown changelog from git history",
        category: "devops",
        content: GIT_CHANGELOG,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "docker-dashboard",
        description: "Collect Docker container, image, and volume statistics",
        category: "devops",
        content: DOCKER_DASHBOARD,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "dependency-audit",
        description: "Run cargo and npm audit in parallel for vulnerability scanning",
        category: "devops",
        content: DEPENDENCY_AUDIT,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "ssl-certificate-checker",
        description: "Inspect SSL certificate expiry and chain for any domain",
        category: "devops",
        content: SSL_CERTIFICATE_CHECKER,
        requires_llm: false,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // CATEGORY: NETWORK (9-12)
    // ═════════════════════════════════════════════════════════════════════════
    ShowcaseWorkflow {
        name: "dns-lookup-chain",
        description: "Perform A, MX, and NS lookups for a domain",
        category: "network",
        content: DNS_LOOKUP_CHAIN,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "website-monitor",
        description: "Check availability and response time of multiple URLs",
        category: "network",
        content: WEBSITE_MONITOR,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "http-header-analyzer",
        description: "Fetch full response headers and analyze security posture",
        category: "network",
        content: HTTP_HEADER_ANALYZER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "ip-geolocation",
        description: "Look up your public IP and geolocate it with timezone info",
        category: "network",
        content: IP_GEOLOCATION,
        requires_llm: false,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // CATEGORY: API (13-16)
    // ═════════════════════════════════════════════════════════════════════════
    ShowcaseWorkflow {
        name: "github-repo-analyzer",
        description: "Fetch stars, forks, issues, and languages for a GitHub repo",
        category: "api",
        content: GITHUB_REPO_ANALYZER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "npm-package-checker",
        description: "Inspect an npm package: version, downloads, dependencies",
        category: "api",
        content: NPM_PACKAGE_CHECKER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "weather-dashboard",
        description: "3-day weather forecast from Open-Meteo (no API key needed)",
        category: "api",
        content: WEATHER_DASHBOARD,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "crates-io-explorer",
        description: "Fetch crate metadata, versions, and download stats from crates.io",
        category: "api",
        content: CRATES_IO_EXPLORER,
        requires_llm: false,
    },
    // ═════════════════════════════════════════════════════════════════════════
    // CATEGORY: DATA (17-20)
    // ═════════════════════════════════════════════════════════════════════════
    ShowcaseWorkflow {
        name: "rss-feed-aggregator",
        description: "Fetch and combine multiple RSS feeds into a unified digest",
        category: "data",
        content: RSS_FEED_AGGREGATOR,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "json-api-explorer",
        description: "Probe multiple API endpoints and combine their data models",
        category: "data",
        content: JSON_API_EXPLORER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "sitemap-crawler",
        description: "Fetch sitemap.xml and check status of discovered URLs",
        category: "data",
        content: SITEMAP_CRAWLER,
        requires_llm: false,
    },
    ShowcaseWorkflow {
        name: "public-api-dashboard",
        description: "Query multiple free public APIs and build a combined dashboard",
        category: "data",
        content: PUBLIC_API_DASHBOARD,
        requires_llm: false,
    },
];

// ═════════════════════════════════════════════════════════════════════════════
// 01: SYSTEM HEALTH DASHBOARD
// ═════════════════════════════════════════════════════════════════════════════

const SYSTEM_HEALTH_DASHBOARD: &str = r##"# System Health Dashboard — collect host metrics into a formatted report
schema: "nika/workflow@0.12"

workflow: system-health-dashboard

tasks:
  - id: hostname
    exec: "hostname"

  - id: uptime
    exec:
      command: "uptime"
      shell: true

  - id: disk
    exec:
      command: "df -h / | tail -1"
      shell: true

  - id: memory
    exec:
      command: "vm_stat 2>/dev/null | head -5 || free -h 2>/dev/null | head -3 || echo 'memory info unavailable'"
      shell: true

  - id: report
    depends_on: [hostname, uptime, disk, memory]
    with:
      host: $hostname
      up: $uptime
      dsk: $disk
      mem: $memory
    exec:
      command: |
        echo "=== SYSTEM HEALTH REPORT ==="
        echo "Host:   {{with.host}}"
        echo "Uptime: {{with.up}}"
        echo "Disk:   {{with.dsk}}"
        echo "Memory: {{with.mem}}"
        echo "=== END REPORT ==="
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 02: ENVIRONMENT REPORTER
// ═════════════════════════════════════════════════════════════════════════════

const ENVIRONMENT_REPORTER: &str = r##"# Environment Reporter — audit env vars grouped by category
schema: "nika/workflow@0.12"

workflow: environment-reporter

tasks:
  - id: shell_info
    exec:
      command: "echo \"SHELL=$SHELL  TERM=$TERM  USER=$USER  HOME=$HOME\""
      shell: true

  - id: path_info
    exec:
      command: "echo $PATH | tr ':' '\n' | head -10"
      shell: true

  - id: dev_tools
    exec:
      command: |
        echo "git: $(git --version 2>/dev/null || echo 'N/A')"
        echo "cargo: $(cargo --version 2>/dev/null || echo 'N/A')"
        echo "node: $(node --version 2>/dev/null || echo 'N/A')"
      shell: true

  - id: summary
    depends_on: [shell_info, path_info, dev_tools]
    with:
      shell: $shell_info
      path: $path_info
      tools: $dev_tools
    exec:
      command: |
        echo "=== ENVIRONMENT AUDIT ==="
        echo "--- Shell ---"
        echo "{{with.shell}}"
        echo "--- PATH (first 10) ---"
        echo "{{with.path}}"
        echo "--- Dev Tools ---"
        echo "{{with.tools}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 03: FILESYSTEM ANALYZER
// ═════════════════════════════════════════════════════════════════════════════

const FILESYSTEM_ANALYZER: &str = r##"# Filesystem Analyzer — project statistics: counts, sizes, lines
schema: "nika/workflow@0.12"

workflow: filesystem-analyzer

tasks:
  - id: file_counts
    exec:
      command: |
        echo "Total files: $(find . -type f -maxdepth 3 2>/dev/null | wc -l | tr -d ' ')"
        echo "Directories: $(find . -type d -maxdepth 3 2>/dev/null | wc -l | tr -d ' ')"
        echo "Rust files:  $(find . -name '*.rs' -maxdepth 5 2>/dev/null | wc -l | tr -d ' ')"
        echo "YAML files:  $(find . -name '*.yaml' -o -name '*.yml' -maxdepth 5 2>/dev/null | wc -l | tr -d ' ')"
      shell: true

  - id: disk_usage
    exec:
      command: "du -sh . 2>/dev/null | cut -f1"
      shell: true

  - id: largest_files
    exec:
      command: "find . -type f -maxdepth 3 -exec ls -la {} + 2>/dev/null | sort -k5 -rn | head -5 | awk '{print $5, $NF}'"
      shell: true

  - id: report
    depends_on: [file_counts, disk_usage, largest_files]
    with:
      counts: $file_counts
      size: $disk_usage
      largest: $largest_files
    exec:
      command: |
        echo "=== FILESYSTEM REPORT ==="
        echo "{{with.counts}}"
        echo "Total size: {{with.size}}"
        echo "--- Largest Files ---"
        echo "{{with.largest}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 04: HOMEBREW AUDIT
// ═════════════════════════════════════════════════════════════════════════════

const HOMEBREW_AUDIT: &str = r##"# Homebrew Audit — check outdated packages and cask status
schema: "nika/workflow@0.12"

workflow: homebrew-audit

tasks:
  - id: brew_version
    exec:
      command: "brew --version 2>/dev/null | head -1 || echo 'Homebrew not installed'"
      shell: true

  - id: outdated_formulae
    exec:
      command: "brew outdated --formula 2>/dev/null | head -15 || echo 'no outdated formulae'"
      shell: true

  - id: outdated_casks
    exec:
      command: "brew outdated --cask 2>/dev/null | head -15 || echo 'no outdated casks'"
      shell: true

  - id: report
    depends_on: [brew_version, outdated_formulae, outdated_casks]
    with:
      version: $brew_version
      formulae: $outdated_formulae
      casks: $outdated_casks
    exec:
      command: |
        echo "=== HOMEBREW AUDIT ==="
        echo "Version: {{with.version}}"
        echo "--- Outdated Formulae ---"
        echo "{{with.formulae}}"
        echo "--- Outdated Casks ---"
        echo "{{with.casks}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 05: GIT CHANGELOG
// ═════════════════════════════════════════════════════════════════════════════

const GIT_CHANGELOG: &str = r##"# Git Changelog — generate a formatted Markdown changelog from git history
schema: "nika/workflow@0.12"

workflow: git-changelog

tasks:
  - id: branch_info
    exec:
      command: "git branch --show-current 2>/dev/null || echo 'detached HEAD'"
      shell: true

  - id: recent_commits
    exec:
      command: "git log --oneline --no-decorate -20 2>/dev/null || echo 'not a git repo'"
      shell: true

  - id: commit_stats
    exec:
      command: "git shortlog -sn --no-merges HEAD~50..HEAD 2>/dev/null | head -10 || echo 'no stats'"
      shell: true

  - id: tags
    exec:
      command: "git tag --sort=-creatordate 2>/dev/null | head -5 || echo 'no tags'"
      shell: true

  - id: changelog
    depends_on: [branch_info, recent_commits, commit_stats, tags]
    with:
      branch: $branch_info
      commits: $recent_commits
      stats: $commit_stats
      recent_tags: $tags
    exec:
      command: |
        echo "=== CHANGELOG ==="
        echo "Branch: {{with.branch}}"
        echo "--- Recent Tags ---"
        echo "{{with.recent_tags}}"
        echo "--- Last 20 Commits ---"
        echo "{{with.commits}}"
        echo "--- Contributors ---"
        echo "{{with.stats}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 06: DOCKER DASHBOARD
// ═════════════════════════════════════════════════════════════════════════════

const DOCKER_DASHBOARD: &str = r##"# Docker Dashboard — container, image, and volume statistics
schema: "nika/workflow@0.12"

workflow: docker-dashboard

tasks:
  - id: docker_version
    exec:
      command: "docker version --format json 2>/dev/null | head -1 || echo 'Docker not installed'"
      shell: true

  - id: running_containers
    exec:
      command: "docker ps 2>/dev/null | head -15 || echo 'no containers running'"
      shell: true

  - id: images
    exec:
      command: "docker images 2>/dev/null | head -15 || echo 'no images'"
      shell: true

  - id: disk_usage
    exec:
      command: "docker system df 2>/dev/null || echo 'Docker disk usage unavailable'"
      shell: true

  - id: dashboard
    depends_on: [docker_version, running_containers, images, disk_usage]
    with:
      version: $docker_version
      containers: $running_containers
      imgs: $images
      disk: $disk_usage
    exec:
      command: |
        echo "=== DOCKER DASHBOARD ==="
        echo "Version: {{with.version}}"
        echo "--- Containers ---"
        echo "{{with.containers}}"
        echo "--- Images ---"
        echo "{{with.imgs}}"
        echo "--- Disk ---"
        echo "{{with.disk}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 07: DEPENDENCY AUDIT
// ═════════════════════════════════════════════════════════════════════════════

const DEPENDENCY_AUDIT: &str = r##"# Dependency Audit — scan for known vulnerabilities in Rust and Node
schema: "nika/workflow@0.12"

workflow: dependency-audit

tasks:
  - id: cargo_audit
    exec:
      command: "cargo audit 2>/dev/null | tail -20 || echo 'cargo-audit not installed (cargo install cargo-audit)'"
      shell: true

  - id: npm_audit
    exec:
      command: "npm audit --json 2>/dev/null | head -30 || echo 'no package.json or npm unavailable'"
      shell: true

  - id: cargo_outdated
    exec:
      command: "cargo outdated -R --depth 1 2>/dev/null | head -20 || echo 'cargo-outdated not installed'"
      shell: true

  - id: report
    depends_on: [cargo_audit, npm_audit, cargo_outdated]
    with:
      rust_audit: $cargo_audit
      node_audit: $npm_audit
      outdated: $cargo_outdated
    exec:
      command: |
        echo "=== DEPENDENCY AUDIT ==="
        echo "--- Rust Security ---"
        echo "{{with.rust_audit}}"
        echo "--- NPM Security ---"
        echo "{{with.node_audit}}"
        echo "--- Outdated Crates ---"
        echo "{{with.outdated}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 08: SSL CERTIFICATE CHECKER
// ═════════════════════════════════════════════════════════════════════════════

const SSL_CERTIFICATE_CHECKER: &str = r##"# SSL Certificate Checker — inspect cert expiry and chain
schema: "nika/workflow@0.12"

workflow: ssl-certificate-checker

tasks:
  - id: cert_info
    exec:
      command: |
        echo | openssl s_client -connect github.com:443 -servername github.com 2>/dev/null | openssl x509 -noout -subject -issuer -dates 2>/dev/null || echo 'openssl unavailable'
      shell: true
      timeout: 15

  - id: cert_chain
    exec:
      command: |
        echo | openssl s_client -connect github.com:443 -servername github.com -showcerts 2>/dev/null | grep -E 'subject=|issuer=' | head -6 || echo 'chain unavailable'
      shell: true
      timeout: 15

  - id: tls_version
    exec:
      command: |
        echo | openssl s_client -connect github.com:443 -servername github.com 2>/dev/null | grep -E 'Protocol|Cipher' | head -3 || echo 'TLS info unavailable'
      shell: true
      timeout: 15

  - id: report
    depends_on: [cert_info, cert_chain, tls_version]
    with:
      cert: $cert_info
      chain: $cert_chain
      tls: $tls_version
    exec:
      command: |
        echo "=== SSL REPORT: github.com ==="
        echo "--- Certificate ---"
        echo "{{with.cert}}"
        echo "--- Chain ---"
        echo "{{with.chain}}"
        echo "--- TLS ---"
        echo "{{with.tls}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 09: DNS LOOKUP CHAIN
// ═════════════════════════════════════════════════════════════════════════════

const DNS_LOOKUP_CHAIN: &str = r##"# DNS Lookup Chain — A, MX, and NS records for a domain
schema: "nika/workflow@0.12"

workflow: dns-lookup-chain

tasks:
  - id: a_records
    exec:
      command: "dig +short github.com A 2>/dev/null || nslookup github.com 2>/dev/null | tail -4"
      shell: true
      timeout: 10

  - id: mx_records
    exec:
      command: "dig +short github.com MX 2>/dev/null || echo 'no MX records'"
      shell: true
      timeout: 10

  - id: ns_records
    exec:
      command: "dig +short github.com NS 2>/dev/null || echo 'no NS records'"
      shell: true
      timeout: 10

  - id: report
    depends_on: [a_records, mx_records, ns_records]
    with:
      a: $a_records
      mx: $mx_records
      ns: $ns_records
    exec:
      command: |
        echo "=== DNS REPORT: github.com ==="
        echo "A Records:  {{with.a}}"
        echo "MX Records: {{with.mx}}"
        echo "NS Records: {{with.ns}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 10: WEBSITE MONITOR
// ═════════════════════════════════════════════════════════════════════════════

const WEBSITE_MONITOR: &str = r##"# Website Monitor — check availability of multiple URLs
schema: "nika/workflow@0.12"

workflow: website-monitor

tasks:
  - id: check_github
    fetch:
      url: "https://github.com"
      response: full
      timeout: 10

  - id: check_crates
    fetch:
      url: "https://crates.io"
      response: full
      timeout: 10

  - id: check_httpbin
    fetch:
      url: "https://httpbin.org/get"
      response: full
      timeout: 10

  - id: check_rust
    fetch:
      url: "https://www.rust-lang.org"
      response: full
      timeout: 10

  - id: dashboard
    depends_on: [check_github, check_crates, check_httpbin, check_rust]
    with:
      github: $check_github | parse_json
      crates: $check_crates | parse_json
      httpbin: $check_httpbin | parse_json
      rust: $check_rust | parse_json
    exec:
      command: |
        echo "=== WEBSITE AVAILABILITY ==="
        echo "github.com    — status: {{with.github.status}}"
        echo "crates.io     — status: {{with.crates.status}}"
        echo "httpbin.org   — status: {{with.httpbin.status}}"
        echo "rust-lang.org — status: {{with.rust.status}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 11: HTTP HEADER ANALYZER
// ═════════════════════════════════════════════════════════════════════════════

const HTTP_HEADER_ANALYZER: &str = r##"# HTTP Header Analyzer — inspect security headers for a URL
schema: "nika/workflow@0.12"

workflow: http-header-analyzer

tasks:
  - id: fetch_headers
    fetch:
      url: "https://github.com"
      response: full
      timeout: 10

  - id: analyze
    depends_on: [fetch_headers]
    with:
      resp: $fetch_headers | parse_json
    exec:
      command: |
        echo "=== HTTP HEADER ANALYSIS: github.com ==="
        echo "Status: {{with.resp.status}}"
        echo "Final URL: {{with.resp.url}}"
        echo "--- Response Headers ---"
        echo "{{with.resp.headers}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 12: IP GEOLOCATION
// ═════════════════════════════════════════════════════════════════════════════

const IP_GEOLOCATION: &str = r##"# IP Geolocation — look up your public IP and geolocate it
schema: "nika/workflow@0.12"

workflow: ip-geolocation

tasks:
  - id: get_ip
    fetch:
      url: "https://httpbin.org/ip"
      extract: jsonpath
      selector: "$.origin"
      timeout: 10

  - id: geo_lookup
    fetch:
      url: "https://ipapi.co/json/"
      timeout: 10

  - id: report
    depends_on: [get_ip, geo_lookup]
    with:
      ip: $get_ip
      geo: $geo_lookup | parse_json
    exec:
      command: |
        echo "=== IP GEOLOCATION ==="
        echo "Public IP:  {{with.ip}}"
        echo "City:       {{with.geo.city}}"
        echo "Region:     {{with.geo.region}}"
        echo "Country:    {{with.geo.country_name}}"
        echo "Timezone:   {{with.geo.timezone}}"
        echo "ISP:        {{with.geo.org}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 13: GITHUB REPO ANALYZER
// ═════════════════════════════════════════════════════════════════════════════

const GITHUB_REPO_ANALYZER: &str = r##"# GitHub Repo Analyzer — fetch stars, forks, issues for a public repo
schema: "nika/workflow@0.12"

workflow: github-repo-analyzer

tasks:
  - id: repo_info
    fetch:
      url: "https://api.github.com/repos/rust-lang/rust"
      headers:
        Accept: "application/vnd.github.v3+json"
        User-Agent: "nika-workflow"
      timeout: 15

  - id: releases
    fetch:
      url: "https://api.github.com/repos/rust-lang/rust/releases?per_page=3"
      headers:
        Accept: "application/vnd.github.v3+json"
        User-Agent: "nika-workflow"
      extract: jsonpath
      selector: "$[*].tag_name"
      timeout: 15

  - id: report
    depends_on: [repo_info, releases]
    with:
      repo: $repo_info | parse_json
      tags: $releases
    exec:
      command: |
        echo "=== GITHUB REPO: rust-lang/rust ==="
        echo "Stars:       {{with.repo.stargazers_count}}"
        echo "Forks:       {{with.repo.forks_count}}"
        echo "Open Issues: {{with.repo.open_issues_count}}"
        echo "Language:    {{with.repo.language}}"
        echo "License:     {{with.repo.license.spdx_id}}"
        echo "Releases:    {{with.tags}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 14: NPM PACKAGE CHECKER
// ═════════════════════════════════════════════════════════════════════════════

const NPM_PACKAGE_CHECKER: &str = r##"# NPM Package Checker — inspect version, downloads, and dependencies
schema: "nika/workflow@0.12"

workflow: npm-package-checker

tasks:
  - id: package_info
    fetch:
      url: "https://registry.npmjs.org/typescript/latest"
      timeout: 10

  - id: download_stats
    fetch:
      url: "https://api.npmjs.org/downloads/point/last-week/typescript"
      timeout: 10

  - id: report
    depends_on: [package_info, download_stats]
    with:
      pkg: $package_info | parse_json
      downloads: $download_stats | parse_json
    exec:
      command: |
        echo "=== NPM PACKAGE: typescript ==="
        echo "Version:     {{with.pkg.version}}"
        echo "Description: {{with.pkg.description}}"
        echo "License:     {{with.pkg.license}}"
        echo "Homepage:    {{with.pkg.homepage}}"
        echo "--- Weekly Downloads ---"
        echo "Downloads: {{with.downloads.downloads}}"
        echo "Period:    {{with.downloads.start}} to {{with.downloads.end}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 15: WEATHER DASHBOARD
// ═════════════════════════════════════════════════════════════════════════════

const WEATHER_DASHBOARD: &str = r##"# Weather Dashboard — 3-day forecast from Open-Meteo (no API key)
schema: "nika/workflow@0.12"

workflow: weather-dashboard

tasks:
  - id: current_weather
    fetch:
      url: "https://api.open-meteo.com/v1/forecast?latitude=48.8566&longitude=2.3522&current=temperature_2m,wind_speed_10m,relative_humidity_2m&timezone=Europe/Paris"
      timeout: 10

  - id: forecast
    fetch:
      url: "https://api.open-meteo.com/v1/forecast?latitude=48.8566&longitude=2.3522&daily=temperature_2m_max,temperature_2m_min,precipitation_sum&timezone=Europe/Paris&forecast_days=3"
      timeout: 10

  - id: report
    depends_on: [current_weather, forecast]
    with:
      current: $current_weather | parse_json
      daily: $forecast | parse_json
    exec:
      command: |
        echo "=== WEATHER: Paris, France ==="
        echo "--- Current ---"
        echo "Temperature: {{with.current.current.temperature_2m}} {{with.current.current_units.temperature_2m}}"
        echo "Wind Speed:  {{with.current.current.wind_speed_10m}} {{with.current.current_units.wind_speed_10m}}"
        echo "Humidity:    {{with.current.current.relative_humidity_2m}}%"
        echo "--- 3-Day Forecast ---"
        echo "Max temps: {{with.daily.daily.temperature_2m_max}}"
        echo "Min temps: {{with.daily.daily.temperature_2m_min}}"
        echo "Rain (mm): {{with.daily.daily.precipitation_sum}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 16: CRATES.IO EXPLORER
// ═════════════════════════════════════════════════════════════════════════════

const CRATES_IO_EXPLORER: &str = r##"# Crates.io Explorer — fetch crate metadata and download stats
schema: "nika/workflow@0.12"

workflow: crates-io-explorer

tasks:
  - id: crate_info
    fetch:
      url: "https://crates.io/api/v1/crates/serde"
      headers:
        User-Agent: "nika-workflow (https://github.com/supernovae-st)"
      timeout: 10

  - id: crate_versions
    fetch:
      url: "https://crates.io/api/v1/crates/serde/versions"
      headers:
        User-Agent: "nika-workflow (https://github.com/supernovae-st)"
      extract: jsonpath
      selector: "$.versions[0:3].num"
      timeout: 10

  - id: report
    depends_on: [crate_info, crate_versions]
    with:
      info: $crate_info | parse_json
      versions: $crate_versions
    exec:
      command: |
        echo "=== CRATE: serde ==="
        echo "Description: {{with.info.crate.description}}"
        echo "Downloads:   {{with.info.crate.downloads}}"
        echo "Max Version: {{with.info.crate.max_version}}"
        echo "Repository:  {{with.info.crate.repository}}"
        echo "--- Latest Versions ---"
        echo "{{with.versions}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 17: RSS FEED AGGREGATOR
// ═════════════════════════════════════════════════════════════════════════════

const RSS_FEED_AGGREGATOR: &str = r##"# RSS Feed Aggregator — fetch and combine multiple RSS feeds
schema: "nika/workflow@0.12"

workflow: rss-feed-aggregator

tasks:
  - id: rust_blog
    fetch:
      url: "https://blog.rust-lang.org/feed.xml"
      extract: feed
      timeout: 15

  - id: github_blog
    fetch:
      url: "https://github.blog/feed/"
      extract: feed
      timeout: 15

  - id: digest
    depends_on: [rust_blog, github_blog]
    with:
      rust: $rust_blog
      github: $github_blog
    exec:
      command: |
        echo "=== RSS FEED DIGEST ==="
        echo "--- Rust Blog ---"
        echo "{{with.rust}}"
        echo "--- GitHub Blog ---"
        echo "{{with.github}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 18: JSON API EXPLORER
// ═════════════════════════════════════════════════════════════════════════════

const JSON_API_EXPLORER: &str = r##"# JSON API Explorer — probe multiple endpoints and map their structures
schema: "nika/workflow@0.12"

workflow: json-api-explorer

tasks:
  - id: github_root
    fetch:
      url: "https://api.github.com"
      headers:
        User-Agent: "nika-workflow"
      timeout: 10

  - id: httpbin_get
    fetch:
      url: "https://httpbin.org/get"
      timeout: 10

  - id: httpbin_headers
    fetch:
      url: "https://httpbin.org/headers"
      timeout: 10

  - id: combined
    depends_on: [github_root, httpbin_get, httpbin_headers]
    with:
      github: $github_root | parse_json
      get_data: $httpbin_get | parse_json
      headers: $httpbin_headers | parse_json
    exec:
      command: |
        echo "=== API EXPLORER ==="
        echo "--- GitHub API Root ---"
        echo "User URL: {{with.github.current_user_url}}"
        echo "Repos URL: {{with.github.repository_url}}"
        echo "--- HTTPBin /get ---"
        echo "Origin: {{with.get_data.origin}}"
        echo "URL: {{with.get_data.url}}"
        echo "--- HTTPBin /headers ---"
        echo "Host: {{with.headers.headers.Host}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 19: SITEMAP CRAWLER
// ═════════════════════════════════════════════════════════════════════════════

const SITEMAP_CRAWLER: &str = r##"# Sitemap Crawler — fetch sitemap.xml and extract URL list
schema: "nika/workflow@0.12"

workflow: sitemap-crawler

tasks:
  - id: fetch_sitemap
    fetch:
      url: "https://httpbin.org/xml"
      timeout: 15

  - id: fetch_robots
    fetch:
      url: "https://github.com/robots.txt"
      timeout: 10

  - id: check_main
    fetch:
      url: "https://httpbin.org"
      response: full
      timeout: 10

  - id: report
    depends_on: [fetch_sitemap, fetch_robots, check_main]
    with:
      sitemap: $fetch_sitemap
      robots: $fetch_robots
      main: $check_main | parse_json
    exec:
      command: |
        echo "=== SITEMAP CRAWL REPORT ==="
        echo "--- XML Content ---"
        echo "{{with.sitemap}}"
        echo "--- robots.txt ---"
        echo "{{with.robots}}"
        echo "--- Main Page ---"
        echo "Status: {{with.main.status}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// 20: PUBLIC API DASHBOARD
// ═════════════════════════════════════════════════════════════════════════════

const PUBLIC_API_DASHBOARD: &str = r##"# Public API Dashboard — query free APIs and build a combined view
schema: "nika/workflow@0.12"

workflow: public-api-dashboard

tasks:
  - id: world_time
    fetch:
      url: "https://worldtimeapi.org/api/timezone/Europe/Paris"
      timeout: 10

  - id: exchange_rates
    fetch:
      url: "https://open.er-api.com/v6/latest/USD"
      extract: jsonpath
      selector: "$.rates.EUR"
      timeout: 10

  - id: fact
    fetch:
      url: "https://uselessfacts.jsph.pl/api/v2/facts/today"
      timeout: 10

  - id: dashboard
    depends_on: [world_time, exchange_rates, fact]
    with:
      time: $world_time | parse_json
      eur_rate: $exchange_rates
      daily_fact: $fact | parse_json
    exec:
      command: |
        echo "=== PUBLIC API DASHBOARD ==="
        echo "--- World Clock (Paris) ---"
        echo "Time: {{with.time.datetime}}"
        echo "Timezone: {{with.time.timezone}}"
        echo "Day of year: {{with.time.day_of_year}}"
        echo "--- Exchange Rate ---"
        echo "1 USD = {{with.eur_rate}} EUR"
        echo "--- Fun Fact of the Day ---"
        echo "{{with.daily_fact.text}}"
      shell: true
"##;

// ═════════════════════════════════════════════════════════════════════════════
// TESTS
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_showcase_exec_count() {
        assert_eq!(
            SHOWCASE_EXEC.len(),
            20,
            "Should have exactly 20 exec/fetch showcase workflows"
        );
    }

    #[test]
    fn test_showcase_exec_names_unique() {
        let mut names: Vec<&str> = SHOWCASE_EXEC.iter().map(|w| w.name).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "All showcase names must be unique");
    }

    #[test]
    fn test_showcase_exec_all_have_schema() {
        for w in SHOWCASE_EXEC {
            assert!(
                w.content.contains("schema: \"nika/workflow@0.12\""),
                "Workflow '{}' must declare schema",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_exec_all_have_workflow_name() {
        for w in SHOWCASE_EXEC {
            assert!(
                w.content.contains("workflow:"),
                "Workflow '{}' must have workflow: declaration",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_exec_all_have_tasks() {
        for w in SHOWCASE_EXEC {
            assert!(
                w.content.contains("tasks:"),
                "Workflow '{}' must have tasks: section",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_exec_no_llm_required() {
        for w in SHOWCASE_EXEC {
            assert!(
                !w.requires_llm,
                "Workflow '{}' should not require an LLM (exec/fetch only)",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_exec_no_infer_verb() {
        for w in SHOWCASE_EXEC {
            assert!(
                !w.content.contains("\n    infer:") && !w.content.contains("\n    infer "),
                "Workflow '{}' must not use infer: verb (exec/fetch only)",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_exec_no_provider_model() {
        for w in SHOWCASE_EXEC {
            assert!(
                !w.content.contains("provider:") && !w.content.contains("model:"),
                "Workflow '{}' must not declare provider/model (no LLM needed)",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_exec_uses_exec_or_fetch() {
        for w in SHOWCASE_EXEC {
            let has_exec = w.content.contains("exec:");
            let has_fetch = w.content.contains("fetch:");
            assert!(
                has_exec || has_fetch,
                "Workflow '{}' must use exec: or fetch: verb",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_exec_uses_depends_on() {
        let with_deps = SHOWCASE_EXEC
            .iter()
            .filter(|w| w.content.contains("depends_on:"))
            .count();
        assert!(
            with_deps >= 18,
            "At least 18 of 20 workflows should use depends_on (got {})",
            with_deps
        );
    }

    #[test]
    fn test_showcase_exec_uses_with_bindings() {
        let with_bindings = SHOWCASE_EXEC
            .iter()
            .filter(|w| w.content.contains("with:"))
            .count();
        assert!(
            with_bindings >= 18,
            "At least 18 of 20 workflows should use with: bindings (got {})",
            with_bindings
        );
    }

    #[test]
    fn test_showcase_exec_valid_yaml() {
        for w in SHOWCASE_EXEC {
            let parsed: Result<serde_json::Value, _> = serde_saphyr::from_str(w.content);
            assert!(
                parsed.is_ok(),
                "Workflow '{}' must be valid YAML: {:?}",
                w.name,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_showcase_exec_categories() {
        let system = SHOWCASE_EXEC
            .iter()
            .filter(|w| w.category == "system")
            .count();
        let devops = SHOWCASE_EXEC
            .iter()
            .filter(|w| w.category == "devops")
            .count();
        let network = SHOWCASE_EXEC
            .iter()
            .filter(|w| w.category == "network")
            .count();
        let api = SHOWCASE_EXEC.iter().filter(|w| w.category == "api").count();
        let data = SHOWCASE_EXEC
            .iter()
            .filter(|w| w.category == "data")
            .count();
        assert_eq!(system, 4, "Should have 4 system workflows");
        assert_eq!(devops, 4, "Should have 4 devops workflows");
        assert_eq!(network, 4, "Should have 4 network workflows");
        assert_eq!(api, 4, "Should have 4 api workflows");
        assert_eq!(data, 4, "Should have 4 data workflows");
    }

    #[test]
    fn test_showcase_exec_descriptions_not_empty() {
        for w in SHOWCASE_EXEC {
            assert!(
                w.description.len() >= 10,
                "Workflow '{}' description too short: '{}'",
                w.name,
                w.description
            );
        }
    }

    #[test]
    fn test_showcase_exec_line_count_range() {
        for w in SHOWCASE_EXEC {
            let lines = w.content.lines().count();
            assert!(
                (15..=50).contains(&lines),
                "Workflow '{}' has {} lines (expected 15-50)",
                w.name,
                lines
            );
        }
    }

    #[test]
    fn test_showcase_exec_all_tasks_have_ids() {
        for w in SHOWCASE_EXEC {
            let task_count = w.content.matches("- id:").count();
            assert!(
                task_count >= 2,
                "Workflow '{}' should have at least 2 tasks with ids (got {})",
                w.name,
                task_count
            );
        }
    }

    #[test]
    fn test_showcase_exec_verb_diversity() {
        let exec_only = SHOWCASE_EXEC
            .iter()
            .filter(|w| w.content.contains("exec:") && !w.content.contains("fetch:"))
            .count();
        let with_fetch = SHOWCASE_EXEC
            .iter()
            .filter(|w| w.content.contains("fetch:"))
            .count();
        assert!(
            exec_only >= 3,
            "Need at least 3 exec-only workflows (got {})",
            exec_only
        );
        assert!(
            with_fetch >= 8,
            "Need at least 8 workflows using fetch (got {})",
            with_fetch
        );
    }
}
