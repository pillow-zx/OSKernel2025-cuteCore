#!/usr/bin/env bash
set -e

echo "[INFO] Setup RISC-V environment (bash only)"

# --------------------------------------------------
# 1. 计算 path
# --------------------------------------------------
BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/test"

echo "[INFO] Base directory: $BASE_DIR"

# --------------------------------------------------
# 2. 定义路径
# --------------------------------------------------
RES_DIR="$BASE_DIR/testsuits-for-oskernel/riscv-syscalls-testing/res"
TOOLCHAIN_TAR="$RES_DIR/kendryte-toolchain-ubuntu-amd64-8.2.0-20190409.tar.xz"
TOOLCHAIN_DIR="$RES_DIR/kendryte-toolchain"
TOOLCHAIN_BIN="$TOOLCHAIN_DIR/bin"

RISCVTESTS_DIR="$BASE_DIR/testsuits-for-oskernel/riscv-syscalls-testing/user"

BASHRC="$HOME/.bashrc"

# --------------------------------------------------
# 3. 解压工具链（仅当未解压）
# --------------------------------------------------
if [[ ! -d "$TOOLCHAIN_DIR" ]]; then
    echo "[INFO] Extracting toolchain..."
    tar -xvf "$TOOLCHAIN_TAR" -C "$RES_DIR"
else
    echo "[INFO] Toolchain already exists, skip extract."
fi

# --------------------------------------------------
# 4. 写入 .bashrc（避免重复）
# --------------------------------------------------
add_if_missing() {
    local line="$1"
    if ! grep -Fq "$line" "$BASHRC" 2>/dev/null; then
        echo "$line" >> "$BASHRC"
        echo "[INFO] Added: $line"
    else
        echo "[INFO] Exists: $line"
    fi
}

echo "[INFO] Updating ~/.bashrc"

add_if_missing "export RISCVTOOLCHAIN=\"$TOOLCHAIN_BIN\""
add_if_missing "export PATH=\"\$RISCVTOOLCHAIN:\$PATH\""
add_if_missing "export RISCVTESTS=\"$RISCVTESTS_DIR\""

# --------------------------------------------------
# 5. 提示
# --------------------------------------------------
echo
echo "[DONE] Environment variables written to ~/.bashrc"
echo "Please run: source ~/.bashrc  (or open a new terminal)"
