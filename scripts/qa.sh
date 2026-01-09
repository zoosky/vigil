#!/bin/bash
# Vigil QA Script - Run before creating PRs
# Usage: ./scripts/qa.sh [--fix]

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

FIX_MODE=false
if [[ "$1" == "--fix" ]]; then
    FIX_MODE=true
fi

echo "========================================"
echo "  Vigil QA Checks"
echo "========================================"
echo ""

# Track failures
FAILED=0

# 1. Format check
echo -n "Checking formatting... "
if $FIX_MODE; then
    cargo fmt --all
    echo -e "${GREEN}formatted${NC}"
else
    if cargo fmt --all -- --check 2>/dev/null; then
        echo -e "${GREEN}OK${NC}"
    else
        echo -e "${RED}FAILED${NC}"
        echo "  Run 'cargo fmt --all' or './scripts/qa.sh --fix' to fix"
        FAILED=1
    fi
fi

# 2. Clippy
echo -n "Running clippy... "
if cargo clippy --all-targets -- -D warnings 2>/dev/null; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    echo "  Fix the clippy warnings above"
    FAILED=1
fi

# 3. Tests
echo -n "Running tests... "
if cargo test --quiet 2>/dev/null; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    echo "  Fix failing tests"
    FAILED=1
fi

# 4. Documentation
echo -n "Building docs... "
if RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --quiet 2>/dev/null; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    echo "  Fix documentation warnings"
    FAILED=1
fi

# 5. Build release (optional but good to verify)
echo -n "Building release... "
if cargo build --release --quiet 2>/dev/null; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC}"
    echo "  Fix build errors"
    FAILED=1
fi

echo ""
echo "========================================"
if [[ $FAILED -eq 0 ]]; then
    echo -e "${GREEN}All checks passed!${NC}"
    echo "Ready to create PR."
    exit 0
else
    echo -e "${RED}Some checks failed.${NC}"
    echo "Fix the issues above before creating PR."
    exit 1
fi
