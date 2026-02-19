#!/bin/bash
set -e

# Get the project root directory
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
PROJECT_ROOT="$DIR/.."

cd "$PROJECT_ROOT"

# 1. Build
echo -e "\033[1;34m🔨 Building Slate (Release)...\033[0m"
cargo build --release

# 2. Get local LAN IP
# Try to find the IP used for internet connection
IP=$(ip route get 1.1.1.1 2>/dev/null | grep -oP 'src \K\S+')
if [ -z "$IP" ]; then
    # Fallback if no internet: try hostname -I
    IP=$(hostname -I | awk '{print $1}')
fi

PORT=8000

# 3. Generate helper script for the ISO
# This allows 'curl | bash' simplicity
cat <<EOF > target/release/serve.sh
#!/bin/bash
set -e
echo "⬇️ Downloading Slate from $IP..."
curl -f -O http://$IP:$PORT/slate
chmod +x slate
echo -e "\033[1;32m✅ Slate Ready!\033[0m"
echo -e "Run: \033[1;37m./slate forge /dev/sdX\033[0m"
EOF

# 4. Print The Instructions
echo ""
echo -e "\033[1;32m🚀 Deployment Server Ready\033[0m"
echo "---------------------------------------------------"
echo "👉 On your Arch Live ISO, run this ONE command:"
echo ""
echo -e "  \033[1;33mcurl -sL http://$IP:$PORT/serve.sh | bash\033[0m"
echo ""
echo "---------------------------------------------------"
echo "Press Ctrl+C to stop server."

# 5. Serve
cd target/release
python3 -m http.server $PORT
