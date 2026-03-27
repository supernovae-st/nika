#!/bin/sh
# Nika CLI installer
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/supernovae-st/nika/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/supernovae-st/nika/main/install.sh | sh -s -- --version v0.42.0
#   curl -fsSL https://raw.githubusercontent.com/supernovae-st/nika/main/install.sh | sh -s -- --install-dir /custom/path
#
# Requirements: curl or wget, tar, shasum/sha256sum
# POSIX-compatible: runs under sh, dash, bash, zsh

set -eu

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

GITHUB_ORG='supernovae-st'
GITHUB_REPO='nika'
GITHUB_API="https://api.github.com/repos/${GITHUB_ORG}/${GITHUB_REPO}"
GITHUB_RELEASES="https://github.com/${GITHUB_ORG}/${GITHUB_REPO}/releases/download"

DEFAULT_INSTALL_DIR="${HOME}/.nika/bin"

# ---------------------------------------------------------------------------
# Colors (disabled if not a terminal or NO_COLOR is set)
# ---------------------------------------------------------------------------

setup_colors() {
  if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    BOLD='\033[1m'
    DIM='\033[2m'
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    CYAN='\033[0;36m'
    RESET='\033[0m'
  else
    BOLD=''
    DIM=''
    RED=''
    GREEN=''
    YELLOW=''
    CYAN=''
    RESET=''
  fi
}

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------

info()    { printf "${CYAN}  >${RESET} %s\n" "$*"; }
success() { printf "${GREEN}  ✓${RESET} %s\n" "$*"; }
warn()    { printf "${YELLOW}  !${RESET} %s\n" "$*" >&2; }
error()   { printf "${RED}  ✗${RESET} %s\n" "$*" >&2; }
step()    { printf "\n${BOLD}%s${RESET}\n" "$*"; }
die()     { error "$*"; exit 1; }

# ---------------------------------------------------------------------------
# Banner
# ---------------------------------------------------------------------------

print_banner() {
  printf '%s\n' "${CYAN}"
  printf '%s\n' '       _  _  _'
  printf '%s\n' '      | \/ || |'
  printf '%s\n' '    __|    || | __  __ _'
  printf '%s\n' '   / _ \ || |/ / / _` |'
  printf '%s\n' '  | | | || |   < | (_| |'
  printf '%s\n' '  |_| |_||_|_|\_\ \__,_|'
  printf '%s\n' ''
  printf '%s\n' '        🦋  SuperNovae Studio'
  printf '%s\n' "${RESET}"
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

INSTALL_VERSION=''
INSTALL_DIR=''

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --version)
        [ "$#" -ge 2 ] || die '--version requires a value (e.g. --version v0.42.0)'
        INSTALL_VERSION="$2"
        shift 2
        ;;
      --version=*)
        INSTALL_VERSION="${1#--version=}"
        shift
        ;;
      --install-dir)
        [ "$#" -ge 2 ] || die '--install-dir requires a value'
        INSTALL_DIR="$2"
        shift 2
        ;;
      --install-dir=*)
        INSTALL_DIR="${1#--install-dir=}"
        shift
        ;;
      -h|--help)
        printf 'Usage: install.sh [--version v0.42.0] [--install-dir /path]\n'
        exit 0
        ;;
      *)
        die "Unknown argument: $1"
        ;;
    esac
  done

  INSTALL_DIR="${INSTALL_DIR:-${DEFAULT_INSTALL_DIR}}"
}

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------

detect_platform() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "${OS}" in
    Darwin)
      OS_NAME='macos'
      ;;
    Linux)
      OS_NAME='linux'
      ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT)
      error 'Windows is not supported by this installer.'
      printf '\n  Alternatives:\n'
      printf '    WSL:   Install via WSL2 then run this script inside Linux\n'
      printf '    Cargo: cargo install nika-cli\n\n'
      exit 1
      ;;
    *)
      die "Unsupported operating system: ${OS}"
      ;;
  esac

  case "${ARCH}" in
    arm64|aarch64)
      ARCH_NAME='arm64'
      ;;
    x86_64|amd64)
      ARCH_NAME='x64'
      ;;
    *)
      die "Unsupported architecture: ${ARCH}. Only arm64 and x86_64 are supported."
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Downloader (curl preferred, wget fallback)
# ---------------------------------------------------------------------------

download() {
  local url="$1"
  local dest="$2"

  if command -v curl > /dev/null 2>&1; then
    curl --fail --silent --location --show-error --output "${dest}" "${url}"
  elif command -v wget > /dev/null 2>&1; then
    wget --quiet --output-document="${dest}" "${url}"
  else
    die 'Neither curl nor wget is available. Please install one and retry.'
  fi
}

download_stdout() {
  local url="$1"

  if command -v curl > /dev/null 2>&1; then
    curl --fail --silent --location --show-error "${url}"
  elif command -v wget > /dev/null 2>&1; then
    wget --quiet --output-document=- "${url}"
  else
    die 'Neither curl nor wget is available. Please install one and retry.'
  fi
}

# ---------------------------------------------------------------------------
# Version resolution
# ---------------------------------------------------------------------------

resolve_version() {
  if [ -n "${INSTALL_VERSION}" ]; then
    info "Pinned to version ${INSTALL_VERSION}"
    return
  fi

  info 'Fetching latest release from GitHub...'
  local api_response
  api_response="$(download_stdout "${GITHUB_API}/releases/latest")" \
    || die "Failed to reach GitHub API. Check your internet connection."

  # Extract tag_name with portable sed (no perl, no python dependency)
  INSTALL_VERSION="$(printf '%s' "${api_response}" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -1)"

  [ -n "${INSTALL_VERSION}" ] \
    || die "Could not parse latest version from GitHub API response."

  success "Latest version: ${INSTALL_VERSION}"
}

# ---------------------------------------------------------------------------
# Checksum verification
# ---------------------------------------------------------------------------

verify_checksum() {
  local file="$1"
  local checksum_file="$2"

  # shasum is macOS/BSD, sha256sum is Linux
  if command -v sha256sum > /dev/null 2>&1; then
    # sha256sum format: "<hash>  <filename>" — check only the hash part
    local expected
    expected="$(awk '{print $1}' "${checksum_file}")"
    local actual
    actual="$(sha256sum "${file}" | awk '{print $1}')"
    [ "${expected}" = "${actual}" ] || die "SHA256 checksum mismatch! Download may be corrupted."
  elif command -v shasum > /dev/null 2>&1; then
    local expected
    expected="$(awk '{print $1}' "${checksum_file}")"
    local actual
    actual="$(shasum -a 256 "${file}" | awk '{print $1}')"
    [ "${expected}" = "${actual}" ] || die "SHA256 checksum mismatch! Download may be corrupted."
  else
    warn 'Neither sha256sum nor shasum found — skipping checksum verification.'
    warn 'Install one of these tools for secure installs.'
  fi
}

# ---------------------------------------------------------------------------
# Download + verify + extract
# ---------------------------------------------------------------------------

install_binary() {
  local asset_name="nika-${OS_NAME}-${ARCH_NAME}-${INSTALL_VERSION}"
  local tarball="${asset_name}.tar.gz"
  local checksum_file="${tarball}.sha256"
  local download_url="${GITHUB_RELEASES}/${INSTALL_VERSION}"

  step 'Downloading'
  info "Asset: ${tarball}"

  download "${download_url}/${tarball}" "${TMP_DIR}/${tarball}" \
    || die "Failed to download ${tarball}. Check the version and try again."
  success 'Tarball downloaded'

  info 'Downloading checksum...'
  if download "${download_url}/${checksum_file}" "${TMP_DIR}/${checksum_file}" 2>/dev/null; then
    step 'Verifying'
    verify_checksum "${TMP_DIR}/${tarball}" "${TMP_DIR}/${checksum_file}"
    success 'Checksum verified'
  else
    warn 'Checksum file not found — skipping verification.'
  fi

  step 'Installing'
  info "Extracting to ${INSTALL_DIR}..."

  mkdir -p "${INSTALL_DIR}"

  # Tarball contains a directory: nika-macos-arm64-VERSION/nika
  # Extract into tmp then move the binary
  tar -xzf "${TMP_DIR}/${tarball}" -C "${TMP_DIR}" \
    || die 'Failed to extract tarball.'

  # Find the extracted binary (handles any prefix directory)
  local binary_path
  binary_path="$(find "${TMP_DIR}" -type f -name 'nika' ! -path '*/\.*' | head -1)"

  [ -n "${binary_path}" ] \
    || die "Binary 'nika' not found inside the tarball."

  # Move binary to install dir
  cp -- "${binary_path}" "${INSTALL_DIR}/nika"
  chmod +x "${INSTALL_DIR}/nika"

  success "Installed nika ${INSTALL_VERSION} to ${INSTALL_DIR}/nika"
}

# ---------------------------------------------------------------------------
# PATH setup instructions
# ---------------------------------------------------------------------------

print_path_instructions() {
  # Check if install dir is already in PATH
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*)
      success "${INSTALL_DIR} is already in PATH"
      return
      ;;
  esac

  warn "${INSTALL_DIR} is not in your PATH."
  printf '\n  Add the following line to your shell config:\n\n'

  # Detect shell for targeted advice
  local shell_name
  shell_name="$(basename "${SHELL:-sh}")"

  case "${shell_name}" in
    fish)
      printf "    ${CYAN}fish_add_path %s${RESET}\n\n" "${INSTALL_DIR}"
      printf '  Or permanently:\n\n'
      printf "    ${CYAN}set -Ux fish_user_paths %s \$fish_user_paths${RESET}\n\n" "${INSTALL_DIR}"
      ;;
    zsh)
      printf "    ${CYAN}echo 'export PATH=\"%s:\$PATH\"' >> ~/.zshrc${RESET}\n\n" "${INSTALL_DIR}"
      printf '  Then reload:\n\n'
      printf "    ${CYAN}source ~/.zshrc${RESET}\n\n"
      ;;
    bash)
      printf "    ${CYAN}echo 'export PATH=\"%s:\$PATH\"' >> ~/.bashrc${RESET}\n\n" "${INSTALL_DIR}"
      printf '  Then reload:\n\n'
      printf "    ${CYAN}source ~/.bashrc${RESET}\n\n"
      ;;
    *)
      printf "    ${CYAN}export PATH=\"%s:\$PATH\"${RESET}\n\n" "${INSTALL_DIR}"
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Post-install summary
# ---------------------------------------------------------------------------

print_summary() {
  local installed_dir="$1"

  printf '\n'
  printf '%s\n' "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
  printf '  %sNika %s installed successfully!%s\n' "${BOLD}" "${INSTALL_VERSION}" "${RESET}"
  printf '%s\n' "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
  printf '\n'
  printf '  %sNext steps:%s\n\n' "${BOLD}" "${RESET}"
  printf '    %s1.%s Initialize your first project:\n\n' "${CYAN}" "${RESET}"
  printf '       %snika init%s\n\n' "${BOLD}" "${RESET}"
  printf '    %s2.%s Or start the interactive TUI:\n\n' "${CYAN}" "${RESET}"
  printf '       %snika ui%s\n\n' "${BOLD}" "${RESET}"
  printf '    %s3.%s Validate an existing workflow:\n\n' "${CYAN}" "${RESET}"
  printf '       %snika check workflow.nika.yaml%s\n\n' "${BOLD}" "${RESET}"
  printf '  %sDocs:%s https://github.com/%s/%s\n\n' \
    "${DIM}" "${RESET}" "${GITHUB_ORG}" "${GITHUB_REPO}"
}

# ---------------------------------------------------------------------------
# Cleanup trap
# ---------------------------------------------------------------------------

TMP_DIR=''

cleanup() {
  if [ -n "${TMP_DIR}" ] && [ -d "${TMP_DIR}" ]; then
    rm -rf -- "${TMP_DIR}"
  fi
}

trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
  setup_colors
  parse_args "$@"

  print_banner

  step 'Detecting platform'
  detect_platform
  success "Detected: ${OS_NAME}/${ARCH_NAME}"

  resolve_version

  TMP_DIR="$(mktemp -d)"

  install_binary

  # Post-install: trigger first-run setup + start daemon in background.
  # Detects editors (VS Code, Cursor, etc.), installs AI rules, shell
  # completions, and starts the daemon for keychain/cache/cron.
  # Runs silently — failures are non-fatal (setup retries on first command).
  step 'Configuring editors and starting daemon'
  if "${INSTALL_DIR}/nika" --quiet daemon start >/dev/null 2>&1; then
    success 'Daemon started — editors and AI tools configured'
  else
    warn 'Background setup skipped (will run on first nika command)'
  fi

  printf '\n'
  print_path_instructions
  print_summary "${INSTALL_DIR}"
}

main "$@"
