#!/bin/bash
# Nika CLI Installer
# Usage: curl -fsSL https://nika.dev/install.sh | bash
#
# Environment variables:
#   NIKA_INSTALL_DIR - Installation directory (default: /usr/local/bin or ~/.local/bin)
#   NIKA_VERSION     - Specific version to install (default: latest)

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

# Config
GITHUB_REPO="supernovae-studio/nika-cli"
BINARY_NAME="nika"

info() {
  printf "${BLUE}info${NC}: %s\n" "$1"
}

success() {
  printf "${GREEN}success${NC}: %s\n" "$1"
}

warn() {
  printf "${YELLOW}warn${NC}: %s\n" "$1"
}

error() {
  printf "${RED}error${NC}: %s\n" "$1" >&2
  exit 1
}

# Detect OS
detect_os() {
  case "$(uname -s)" in
    Linux*)  echo "linux" ;;
    Darwin*) echo "darwin" ;;
    MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
    *) error "Unsupported operating system: $(uname -s)" ;;
  esac
}

# Detect architecture
detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo "x86_64" ;;
    arm64|aarch64) echo "aarch64" ;;
    *) error "Unsupported architecture: $(uname -m)" ;;
  esac
}

# Get latest version from GitHub
get_latest_version() {
  curl -fsSL "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" \
    | grep '"tag_name":' \
    | sed -E 's/.*"([^"]+)".*/\1/'
}

# Determine install directory
get_install_dir() {
  if [ -n "${NIKA_INSTALL_DIR:-}" ]; then
    echo "$NIKA_INSTALL_DIR"
  elif [ -w "/usr/local/bin" ]; then
    echo "/usr/local/bin"
  else
    echo "$HOME/.local/bin"
  fi
}

# Main installation
main() {
  printf "\n"
  printf "${PURPLE}  ███╗   ██╗██╗██╗  ██╗ █████╗ ${NC}\n"
  printf "${PURPLE}  ████╗  ██║██║██║ ██╔╝██╔══██╗${NC}\n"
  printf "${PURPLE}  ██╔██╗ ██║██║█████╔╝ ███████║${NC}\n"
  printf "${PURPLE}  ██║╚██╗██║██║██╔═██╗ ██╔══██║${NC}\n"
  printf "${PURPLE}  ██║ ╚████║██║██║  ██╗██║  ██║${NC}\n"
  printf "${PURPLE}  ╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝${NC}\n"
  printf "\n"
  printf "  Native Intelligence Kernel for Agents\n"
  printf "\n"

  OS=$(detect_os)
  ARCH=$(detect_arch)
  VERSION="${NIKA_VERSION:-$(get_latest_version)}"
  INSTALL_DIR=$(get_install_dir)

  info "Detected: ${OS}/${ARCH}"
  info "Version: ${VERSION}"
  info "Install directory: ${INSTALL_DIR}"

  # Construct download URL
  if [ "$OS" = "windows" ]; then
    ARCHIVE_NAME="nika-${VERSION}-${ARCH}-pc-windows-msvc.zip"
  else
    TARGET="${ARCH}-unknown-${OS}-gnu"
    if [ "$OS" = "darwin" ]; then
      TARGET="${ARCH}-apple-darwin"
    fi
    ARCHIVE_NAME="nika-${VERSION}-${TARGET}.tar.gz"
  fi

  DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/${VERSION}/${ARCHIVE_NAME}"

  # Create temp directory
  TMP_DIR=$(mktemp -d)
  trap "rm -rf ${TMP_DIR}" EXIT

  info "Downloading ${ARCHIVE_NAME}..."

  if ! curl -fsSL "$DOWNLOAD_URL" -o "${TMP_DIR}/${ARCHIVE_NAME}"; then
    error "Failed to download from ${DOWNLOAD_URL}"
  fi

  info "Extracting..."

  if [ "$OS" = "windows" ]; then
    unzip -q "${TMP_DIR}/${ARCHIVE_NAME}" -d "${TMP_DIR}"
  else
    tar -xzf "${TMP_DIR}/${ARCHIVE_NAME}" -C "${TMP_DIR}"
  fi

  # Create install directory if needed
  mkdir -p "$INSTALL_DIR"

  # Install binary
  info "Installing to ${INSTALL_DIR}/${BINARY_NAME}..."

  if [ "$OS" = "windows" ]; then
    cp "${TMP_DIR}/${BINARY_NAME}.exe" "${INSTALL_DIR}/"
  else
    cp "${TMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
  fi

  # Verify installation
  if command -v "$BINARY_NAME" &> /dev/null; then
    success "Nika CLI installed successfully!"
    printf "\n"
    "$BINARY_NAME" --version
  else
    warn "Nika installed to ${INSTALL_DIR}, but it's not in your PATH."
    printf "\n"
    printf "Add this to your shell profile:\n"
    printf "\n"
    printf "  ${YELLOW}export PATH=\"\$PATH:${INSTALL_DIR}\"${NC}\n"
    printf "\n"
  fi

  printf "\n"
  printf "Get started:\n"
  printf "  ${GREEN}nika init my-project${NC}     # Create new project\n"
  printf "  ${GREEN}nika validate${NC}            # Validate workflows\n"
  printf "  ${GREEN}nika run workflow.wf.yaml${NC} # Run a workflow\n"
  printf "\n"
  printf "Documentation: ${BLUE}https://nika.dev/docs${NC}\n"
  printf "\n"
}

main "$@"
