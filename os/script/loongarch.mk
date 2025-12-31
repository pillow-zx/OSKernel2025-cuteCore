TARGET := loongarch64-unknown-none
MODE := release
KERNEL_ELF := target/loongarch64-unknown-none/$(MODE)/os
KERNEL_BIN := $(KERNEL_ELF).bin

BOARD := 2k1000
SBI ?=
BOOTLOADER := ../bootloader/u-boot-with-spl.bin

BOADR ?= laqemu

OBJCOPY := loongarch64-linux-gnu-objcopy
OBJDUMP := loongarch64-linux-gnu-objdump
READELF := loongarch64-linux-gnu-readelf

build: $(KERNEL_BIN) mv

mv:
	@cp $(KERNEL_BIN) ../kernel-qemu

$(KERNEL_BIN): kernel
	@$(OBJCOPY) ${KERNEL_ELF} --strip-all -O binary $@


kernel:
	@echo Platform: $(BOARD), SBI: $(SBI)
	@cp src/hal/arch/loongarch/linker-$(BOARD).ld src/hal/arch/loongarch/linker.ld
	@LOG=${LOG} cargo build --${MODE} --target $(TARGET) --features "board_$(BOARD)"

run:
	qemu-system-loongarch64 \
	-M ls2k \
	-serial stdio \
	-serial vc	\
 	-drive if=pflash,file=/tmp/qemu/2k1000/u-boot-with-spl.bin \
	-m 1024 \
 	-device usb-kbd,bus=usb-bus.0 \
 	-device usb-tablet,bus=usb-bus.0 \
 	-device usb-storage,drive=udisk \
    -drive if=none,id=udisk,file=/tmp/disk \
 	-net nic \
	-net user,net=10.0.2.0/24,tftp=/srv/tftp \
 	-vnc :0 -D /tmp/qemu.log -s -hda /tmp/qemu/2k1000/2kfs.img