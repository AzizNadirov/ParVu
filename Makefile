.PHONY: help install dev build beta test lint clean

help:
	@echo "ParVu"
	@echo ""
	@echo "  make install   Install UI packages"
	@echo "  make dev       Run the app with live UI reloading"
	@echo "  make build     Build the installer (package 'parvu')"
	@echo "  make beta      Build the beta installer (package 'parvu-next')"
	@echo "  make test      Run Rust and UI tests"
	@echo "  make lint      Run clippy and the UI typecheck"
	@echo "  make clean     Remove build output"

install:
	npm --prefix ui ci

dev: install
	cd crates/parvu-app && npx --prefix ../../ui tauri dev

build:
	./build.sh

beta:
	./build.sh --beta

test:
	cargo test -p parvu-core -p parvu-dsl -p parvu-config
	npm --prefix ui test

lint:
	cargo clippy --workspace --all-targets -- -D warnings
	npm --prefix ui run typecheck

clean:
	cargo clean
	rm -rf ui/dist
