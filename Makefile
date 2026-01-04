# Convenience wrapper for CMake

BUILD_DIR := build
BUILD_TYPE ?= Release

.PHONY: all run clean rebuild

all:
	@cmake -B $(BUILD_DIR) -DCMAKE_BUILD_TYPE=$(BUILD_TYPE)
	@cmake --build $(BUILD_DIR) -j

run: all
	@./$(BUILD_DIR)/kream_engine

clean:
	@rm -rf $(BUILD_DIR)

rebuild: clean all
