.PHONY: build install uninstall clean help release debug

BINARY_NAME = hp-instant-ink-cli
INSTALL_DIR ?= /usr/local/bin
CARGO = cargo

help:
	@echo "HP Instant Ink CLI - Available targets:"
	@echo "  build      - Build the binary in release mode"
	@echo "  debug      - Build the binary in debug mode"
	@echo "  install    - Build and install the binary to $(INSTALL_DIR)"
	@echo "  uninstall  - Remove the binary from $(INSTALL_DIR)"
	@echo "  clean      - Clean build artifacts"
	@echo "  test       - Run tests"
	@echo ""
	@echo "Environment variables:"
	@echo "  INSTALL_DIR - Installation directory (default: /usr/local/bin)"

build: release

release:
	$(CARGO) build --release

debug:
	$(CARGO) build

test:
	$(CARGO) test

install: release
	@if [ ! -w "$(INSTALL_DIR)" ]; then \
		echo "Installing $(BINARY_NAME) to $(INSTALL_DIR) (requires sudo)..."; \
		sudo cp target/release/$(BINARY_NAME) $(INSTALL_DIR)/$(BINARY_NAME); \
		sudo chmod +x $(INSTALL_DIR)/$(BINARY_NAME); \
	else \
		echo "Installing $(BINARY_NAME) to $(INSTALL_DIR)..."; \
		cp target/release/$(BINARY_NAME) $(INSTALL_DIR)/$(BINARY_NAME); \
		chmod +x $(INSTALL_DIR)/$(BINARY_NAME); \
	fi
	@echo "Installation complete!"
	@echo "You can now run: $(BINARY_NAME) --help"

uninstall:
	@if [ -f "$(INSTALL_DIR)/$(BINARY_NAME)" ]; then \
		if [ ! -w "$(INSTALL_DIR)" ]; then \
			echo "Removing $(BINARY_NAME) from $(INSTALL_DIR) (requires sudo)..."; \
			sudo rm $(INSTALL_DIR)/$(BINARY_NAME); \
		else \
			echo "Removing $(BINARY_NAME) from $(INSTALL_DIR)..."; \
			rm $(INSTALL_DIR)/$(BINARY_NAME); \
		fi; \
		echo "$(BINARY_NAME) has been uninstalled successfully!"; \
	else \
		echo "$(BINARY_NAME) is not installed at $(INSTALL_DIR)/$(BINARY_NAME)"; \
		exit 1; \
	fi

clean:
	$(CARGO) clean
