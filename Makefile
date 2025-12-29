.PHONY: help install build clean run test deb

# Default target
help:
	@echo "ParVu Build System"
	@echo "=================="
	@echo ""
	@echo "Available targets:"
	@echo "  make install      - Install dependencies (including build tools)"
	@echo "  make build        - Build standalone binary"
	@echo "  make deb          - Build both binary and .deb package (Linux only)"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make run          - Run the application from source"
	@echo "  make test         - Test the built application"
	@echo ""
	@echo "Platform-specific:"
	@echo "  Linux:   ./build.sh     - Creates binary and .deb"
	@echo "  Windows: .\\build.ps1    - Creates portable and installer"

install:
	@echo "Installing dependencies..."
	uv sync --extra build

build:
	@echo "Building ParVu with PyInstaller..."
	uv run pyinstaller parvu.spec --clean --noconfirm

deb: build
	@echo "Creating .deb package..."
	@./build.sh

clean:
	@echo "Cleaning build artifacts..."
	rm -rf build dist *.spec~
	rm -f ParVu*.deb ParVu*.zip
	find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete

run:
	@echo "Running ParVu from source..."
	uv run python src/app.py

test:
	@if [ -d "dist/parvu" ]; then \
		echo "Testing built application..."; \
		cd dist/parvu && ./parvu; \
	else \
		echo "Error: No build found. Run 'make build' first."; \
		exit 1; \
	fi
