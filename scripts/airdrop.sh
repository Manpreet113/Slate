#!/bin/bash
set -e

# Get project root
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
PROJECT_ROOT="$DIR/.."
cd "$PROJECT_ROOT"

# 1. Build
echo -e "\033[1;34m🔨 Building Slate (Release)...\033[0m"
cargo build --release -q

# 2. Upload to 0x0.st (The Null Pointer)
echo -e "\033[1;34m☁️  Uploading to the void (0x0.st)...\033[0m"
URL=$(curl -F "file=@target/release/slate" https://0x0.st)

# Trim whitespace
URL=$(echo "$URL" | tr -d '[:space:]')

# 3. Print Instructions
echo ""
echo -e "\033[1;32m✅ Air Drop Successful\033[0m"
echo "---------------------------------------------------"
echo "👉 On the Target Machine:"
echo ""
echo -e "  \033[1;33mcurl -L $URL > slate && chmod +x slate && ./slate forge /dev/sdX\033[0m"
echo ""
echo "---------------------------------------------------"
echo "Link expires based on file size (usually weeks)."
