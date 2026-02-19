#!/bin/bash
# Pre-push tag check - Proposes version tagging based on commits since last tag
# Usage: Called automatically before git push OR manually via /release skill

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Get last tag
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BOLD}📦 Release Check${NC} | Last tag: ${YELLOW}${LAST_TAG}${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Count commits since last tag
COMMIT_COUNT=$(git rev-list ${LAST_TAG}..HEAD --count 2>/dev/null || git rev-list HEAD --count)

if [ "$COMMIT_COUNT" -eq 0 ]; then
    echo -e "${GREEN}✓ No new commits since ${LAST_TAG}${NC}"
    exit 0
fi

echo -e "\n${BOLD}📝 ${COMMIT_COUNT} commits since ${LAST_TAG}:${NC}\n"

# Analyze commit types
BREAKING=$(git log ${LAST_TAG}..HEAD --oneline 2>/dev/null | grep -ciE '(BREAKING|!)' || echo 0)
FEATURES=$(git log ${LAST_TAG}..HEAD --oneline 2>/dev/null | grep -ci '^[a-f0-9]* feat' || echo 0)
FIXES=$(git log ${LAST_TAG}..HEAD --oneline 2>/dev/null | grep -ci '^[a-f0-9]* fix' || echo 0)
CI_CHANGES=$(git log ${LAST_TAG}..HEAD --oneline 2>/dev/null | grep -ci '^[a-f0-9]* ci' || echo 0)
TESTS=$(git log ${LAST_TAG}..HEAD --oneline 2>/dev/null | grep -ci '^[a-f0-9]* test' || echo 0)
DOCS=$(git log ${LAST_TAG}..HEAD --oneline 2>/dev/null | grep -ci '^[a-f0-9]* docs' || echo 0)
CHORE=$(git log ${LAST_TAG}..HEAD --oneline 2>/dev/null | grep -ci '^[a-f0-9]* chore' || echo 0)

# Show recent commits
git log ${LAST_TAG}..HEAD --oneline --no-decorate | head -10 | while read line; do
    if echo "$line" | grep -qiE 'feat'; then
        echo -e "  ${GREEN}▸${NC} $line"
    elif echo "$line" | grep -qiE 'fix'; then
        echo -e "  ${YELLOW}▸${NC} $line"
    elif echo "$line" | grep -qiE 'ci|test'; then
        echo -e "  ${BLUE}▸${NC} $line"
    else
        echo -e "  ${NC}▸ $line"
    fi
done

if [ "$COMMIT_COUNT" -gt 10 ]; then
    echo -e "  ${NC}... and $((COMMIT_COUNT - 10)) more commits"
fi

# Summary
echo -e "\n${BOLD}📊 Summary:${NC}"
echo -e "  ${GREEN}feat:${NC} $FEATURES | ${YELLOW}fix:${NC} $FIXES | ${BLUE}ci:${NC} $CI_CHANGES | ${CYAN}test:${NC} $TESTS | docs: $DOCS | chore: $CHORE"

if [ "$BREAKING" -gt 0 ]; then
    echo -e "  ${RED}⚠️  BREAKING CHANGES: $BREAKING${NC}"
fi

# Suggest version bump
echo -e "\n${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Parse current version
IFS='.' read -r MAJOR MINOR PATCH <<< "${LAST_TAG#v}"
PATCH=${PATCH%%[^0-9]*}  # Remove any suffix like -beta

# Determine suggested bump
if [ "$BREAKING" -gt 0 ]; then
    SUGGESTED="major"
    NEW_MAJOR=$((MAJOR + 1))
    SUGGESTED_VERSION="v${NEW_MAJOR}.0.0"
elif [ "$FEATURES" -gt 0 ]; then
    SUGGESTED="minor"
    NEW_MINOR=$((MINOR + 1))
    SUGGESTED_VERSION="v${MAJOR}.${NEW_MINOR}.0"
elif [ "$FIXES" -gt 0 ] || [ "$CI_CHANGES" -gt 0 ]; then
    SUGGESTED="patch"
    NEW_PATCH=$((PATCH + 1))
    SUGGESTED_VERSION="v${MAJOR}.${MINOR}.${NEW_PATCH}"
else
    SUGGESTED="patch"
    NEW_PATCH=$((PATCH + 1))
    SUGGESTED_VERSION="v${MAJOR}.${MINOR}.${NEW_PATCH}"
fi

echo -e "${BOLD}🏷️  Tag Options:${NC}\n"
echo -e "  ${GREEN}[1]${NC} ${SUGGESTED_VERSION} (${SUGGESTED}) ${YELLOW}← Recommended${NC}"

# Calculate other options
NEW_PATCH_V="v${MAJOR}.${MINOR}.$((PATCH + 1))"
NEW_MINOR_V="v${MAJOR}.$((MINOR + 1)).0"
NEW_MAJOR_V="v$((MAJOR + 1)).0.0"

if [ "$SUGGESTED" != "patch" ]; then
    echo -e "  ${NC}[2] ${NEW_PATCH_V} (patch)"
fi
if [ "$SUGGESTED" != "minor" ]; then
    echo -e "  ${NC}[3] ${NEW_MINOR_V} (minor)"
fi
if [ "$SUGGESTED" != "major" ]; then
    echo -e "  ${NC}[4] ${NEW_MAJOR_V} (major)"
fi
echo -e "  ${NC}[5] Custom version"
echo -e "  ${NC}[s] Skip - push without tagging"
echo -e "  ${NC}[c] Cancel push"

echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Export for use by calling script
export SUGGESTED_VERSION
export LAST_TAG
export COMMIT_COUNT
export FEATURES
export FIXES
export BREAKING
