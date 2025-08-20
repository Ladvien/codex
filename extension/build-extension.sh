#!/bin/bash
# Build script for Codex Memory Claude Desktop Extension

set -e

echo "Building Codex Memory Desktop Extension..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get the project root (parent of extension directory)
EXTENSION_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$EXTENSION_DIR")"
BUILD_DIR="$EXTENSION_DIR/build"

echo -e "${YELLOW}Project root: $PROJECT_ROOT${NC}"
echo -e "${YELLOW}Extension dir: $EXTENSION_DIR${NC}"

# Clean and create build directory
echo "Preparing build directory..."
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# Build the Rust binary in release mode
echo -e "${GREEN}Building Rust binary...${NC}"
cd "$PROJECT_ROOT"
cargo build --release

# Copy the binary to the build directory
echo "Copying binary to build directory..."
cp "$PROJECT_ROOT/target/release/codex-memory" "$BUILD_DIR/"

# Copy extension files
echo "Copying extension files..."
cp "$EXTENSION_DIR/manifest.json" "$BUILD_DIR/"
cp "$EXTENSION_DIR/run-codex.sh" "$BUILD_DIR/"

# Create an icon if it doesn't exist (placeholder)
if [ ! -f "$EXTENSION_DIR/icon.png" ]; then
    echo -e "${YELLOW}Creating placeholder icon...${NC}"
    # Create a simple 128x128 placeholder icon using ImageMagick if available
    if command -v convert &> /dev/null; then
        convert -size 128x128 xc:blue -fill white -gravity center \
                -pointsize 72 -annotate +0+0 'C' "$BUILD_DIR/icon.png"
    else
        echo -e "${YELLOW}ImageMagick not found, skipping icon creation${NC}"
        # Create empty icon file as placeholder
        touch "$BUILD_DIR/icon.png"
    fi
else
    cp "$EXTENSION_DIR/icon.png" "$BUILD_DIR/"
fi

# Set executable permissions
echo -e "${GREEN}Setting executable permissions...${NC}"
chmod +x "$BUILD_DIR/codex-memory"
chmod +x "$BUILD_DIR/run-codex.sh"

# Package the extension
echo -e "${GREEN}Packaging extension...${NC}"
cd "$BUILD_DIR"

# Check if dxt tool is available
if command -v dxt &> /dev/null; then
    echo "Using dxt tool to create extension package..."
    dxt pack
    mv *.dxt "$EXTENSION_DIR/codex-memory.dxt"
    echo -e "${GREEN}Extension packaged as: $EXTENSION_DIR/codex-memory.dxt${NC}"
else
    echo -e "${YELLOW}dxt tool not found, creating manual ZIP package...${NC}"
    echo "You can install dxt with: npm install -g @anthropic-ai/dxt"
    
    # Create ZIP package manually
    zip -r "../codex-memory.dxt" .
    echo -e "${GREEN}Extension packaged as: $EXTENSION_DIR/codex-memory.dxt${NC}"
fi

# Clean up build directory
echo "Cleaning up..."
cd "$EXTENSION_DIR"
rm -rf "$BUILD_DIR"

echo -e "${GREEN}✓ Extension build complete!${NC}"
echo ""
echo "To install in Claude Desktop:"
echo "1. Open Claude Desktop"
echo "2. Go to Settings > Extensions"
echo "3. Click 'Install Extension'"
echo "4. Select: $EXTENSION_DIR/codex-memory.dxt"
echo ""
echo "For development/testing, you can also:"
echo "- Use 'dxt install $EXTENSION_DIR/codex-memory.dxt' if you have dxt CLI"
echo "- Or manually extract the .dxt file to Claude's extension directory"