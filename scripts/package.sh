#!/bin/bash
# scrcpy-mac-ui packaging script
# Generates a distributable installation package

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}📦 scrcpy-mac-ui Packaging Script${NC}"
echo ""

# Detect project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Get version from Cargo.toml
VERSION=$(grep "^version" src-tauri/Cargo.toml | head -1 | awk -F'"' '{print $2}')
echo -e "${GREEN}✓${NC} Version: $VERSION"

# Check if app exists
APP_PATH="src-tauri/target/release/bundle/macos/scrcpy-mac-ui.app"
if [ ! -d "$APP_PATH" ]; then
    echo -e "${RED}✗${NC} App not found at: $APP_PATH"
    echo -e "${YELLOW}→${NC} Run 'bun run tauri build' first"
    exit 1
fi
echo -e "${GREEN}✓${NC} Found app: $APP_PATH"

# Create dist directory
DIST_NAME="scrcpy-mac-ui-installer-v${VERSION}"
DIST_DIR="/tmp/${DIST_NAME}"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# Copy app
echo -e "${GREEN}→${NC} Copying app..."
cp -r "$APP_PATH" "$DIST_DIR/"

# Create install_deps.sh
echo -e "${GREEN}→${NC} Generating install_deps.sh..."
cat > "$DIST_DIR/install_deps.sh" << 'EOF'
#!/bin/bash
# scrcpy-mac-ui Dependency Installer
# Installs adb and scrcpy via Homebrew

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}🔧 scrcpy-mac-ui Dependency Installer${NC}"
echo ""

# Check Homebrew
if ! command -v brew &> /dev/null; then
    echo -e "${YELLOW}⚠️  Homebrew not found. Installing...${NC}"
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
else
    echo -e "${GREEN}✓${NC} Homebrew installed"
fi

# Install adb
if ! command -v adb &> /dev/null; then
    echo -e "${YELLOW}→${NC} Installing adb (android-platform-tools)..."
    brew install android-platform-tools
else
    echo -e "${GREEN}✓${NC} adb already installed"
fi

# Install scrcpy
if ! command -v scrcpy &> /dev/null; then
    echo -e "${YELLOW}→${NC} Installing scrcpy..."
    brew install scrcpy
else
    echo -e "${GREEN}✓${NC} scrcpy already installed"
fi

echo ""
echo -e "${GREEN}✅ All dependencies installed!${NC}"
echo ""
echo "Next steps:"
echo "1. Drag scrcpy-mac-ui.app to /Applications/"
echo "2. Connect your Android device via USB"
echo "3. Enable USB debugging on device"
echo "4. Launch scrcpy-mac-ui"
EOF

chmod +x "$DIST_DIR/install_deps.sh"

# Create README
echo -e "${GREEN}→${NC} Generating README.md..."
cat > "$DIST_DIR/README.md" << 'EOF'
# scrcpy-mac-ui 安装指南

## 快速开始（3 步）

### 1. 安装依赖
```bash
bash install_deps.sh
```

这会通过 Homebrew 安装：
- `adb`（Android Debug Bridge）
- `scrcpy`（屏幕镜像工具）

### 2. 安装应用
将 `scrcpy-mac-ui.app` 拖到 `/Applications/` 文件夹

或者直接双击运行（无需安装）

### 3. 连接设备
1. 用 USB 线连接 Android 设备到 Mac
2. 在设备上启用「USB 调试」（开发者选项）
3. 授权电脑访问（设备会弹窗）
4. 打开 scrcpy-mac-ui，点击「高画质启动」

---

## 首次配置

### Mac 键盘输入（推荐）

投屏后，按 `⌘K` 打开手机物理键盘设置：
1. 手机会自动跳转到「设置 → 物理键盘」
2. 点击 "English (US)" 或 "简体中文"
3. 返回聊天界面，Mac 键盘即可输入

**注意：** Mac 需保持 **ABC 英文**输入法，中文由手机输入法处理。

### 无线投屏（可选）

1. 首次连接使用 USB 线
2. 点击 app 中的「USB 启用无线」
3. 拔掉 USB 线
4. 下次可以无线连接（设备需在同一 Wi-Fi）

---

## 常见问题

### 设备未检测到

**检查清单：**
- ✅ USB 调试已启用（设置 → 开发者选项）
- ✅ 授权弹窗已点击「允许」
- ✅ USB 线支持数据传输（不是仅充电线）

**验证命令：**
```bash
adb devices
```

应显示设备序列号 + `device` 状态。

如果显示 `unauthorized`，在手机上重新授权。

### App 无法打开（macOS 阻止）

首次打开会被 macOS 拦截（未签名应用）：

**方法 1：** 右键点击 app → 选择「打开」

**方法 2：** 系统设置 → 隐私与安全性 → 拉到底部点击「仍要打开」

### scrcpy 窗口闪退

**可能原因：**
- scrcpy 版本过旧（需要 ≥4.0）
- 设备不支持某些编码格式

**解决方案：**
```bash
brew upgrade scrcpy
```

---

## 技术支持

**项目地址：** https://github.com/your-repo/scrcpy-mac-ui

**问题反馈：** https://github.com/your-repo/scrcpy-mac-ui/issues

**依赖项：**
- [scrcpy](https://github.com/Genymobile/scrcpy) (≥4.0)
- [adb](https://developer.android.com/tools/adb)
EOF

echo -e "${GREEN}→${NC} Creating zip archive..."
cd /tmp
zip -r "${DIST_NAME}.zip" "${DIST_NAME}" > /dev/null

# Output results
OUTPUT_ZIP="/tmp/${DIST_NAME}.zip"
echo ""
echo -e "${GREEN}✅ Package created successfully!${NC}"
echo ""
echo "📦 Package: $OUTPUT_ZIP"
echo "📊 Size: $(du -h "$OUTPUT_ZIP" | awk '{print $1}')"
echo ""
echo "Contents:"
echo "  - scrcpy-mac-ui.app"
echo "  - install_deps.sh"
echo "  - README.md"
echo ""
echo -e "${YELLOW}Next step:${NC} Share ${DIST_NAME}.zip with users"
