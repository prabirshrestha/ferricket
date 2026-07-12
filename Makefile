.DEFAULT_GOAL := help

CARGO ?= cargo

.PHONY: help build check clean install lock uninstall

help:
	@printf '%s\n' \
		'make build      Build the Bun frontend and Rust CLI' \
		'make check      Run frontend and Rust verification' \
		'make clean      Remove local build artifacts' \
		'make install    Build and install fer into the local Cargo bin directory' \
		'make lock       Refresh Ferricket workspace versions in Cargo.lock' \
		'make uninstall  Remove fer from the local Cargo bin directory'

build:
	$(CARGO) x build

check:
	$(CARGO) x check

clean:
	$(CARGO) x clean

install:
	$(CARGO) x install

lock:
	$(CARGO) x lock

uninstall:
	$(CARGO) x uninstall
