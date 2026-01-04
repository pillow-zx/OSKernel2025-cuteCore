#!/bin/bash

U_FS_DIR="../fs-img"
U_FS="fs.img"
TEST_DIR="../user/src/bin"
ELF_DIR="../user/target/riscv64gc-unknown-none-elf/release"

# 使用标准的 512 字节扇区，增加 count 以满足 FAT32 最小容量限制
# 64MB 镜像 = 128 * 1024 * 512 字节
BLK_SZ="512"
COUNT="131072" 

mkdir -p ${U_FS_DIR}

# 1. 创建镜像
dd if=/dev/zero of=${U_FS_DIR}/${U_FS} bs=${BLK_SZ} count=${COUNT}

# 2. 格式化为 FAT32 
# -F 32 指定 FAT32, -s 8 表示每个簇 8 个扇区 (4KB per cluster)
mkfs.vfat -F 32 ${U_FS_DIR}/${U_FS}

# 3. 创建 bin 目录
mmd -i ${U_FS_DIR}/${U_FS} ::/bin

# 4. 循环拷贝 ELF 文件
for program_rs in $(ls ${TEST_DIR}); do
    # 移除 .rs 后缀获取二进制文件名
    program_name=${program_rs%.rs}

    # 检查 ELF 文件是否存在再拷贝
    if [ -f "${ELF_DIR}/${program_name}" ]; then
        echo "Copying ${program_name} to image..."
        mcopy -i ${U_FS_DIR}/${U_FS} "${ELF_DIR}/${program_name}" ::/
    else
        echo "Warning: ${program_name} not found in ${ELF_DIR}"
    fi
done

for program in $(ls ../test/testsuits-for-oskernel/riscv-syscalls-testing/user/riscv64); do
    mcopy -i ${U_FS_DIR}/${U_FS} ../test/testsuits-for-oskernel/riscv-syscalls-testing/user/riscv64/${program} ::/
done

echo "DONE"