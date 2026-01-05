all: pre build run

pre:
	@cd ${RISCVTESTS} && bash ./build-oscomp.sh

build:
	@cd os && make build

run:
	@cd os && make run

clean:
	@cd os && make clean
	@cd ${RISCVTESTS} && make clean