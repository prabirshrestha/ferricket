.DEFAULT_GOAL := help

CARGO ?= cargo

.PHONY: help build check clean install uninstall

help:
	@printf '%s\n' \
		'make build      Build the Bun frontend and Rust CLI' \
		'make check      Run frontend and Rust verification' \
		'make clean      Remove local build artifacts' \
		'make install    Build and install fer into the local Cargo bin directory' \
		'make uninstall  Remove fer from the local Cargo bin directory'

build:
	$(CARGO) x build

check:
	$(CARGO) x check

clean:
	$(CARGO) x clean

install:
	$(CARGO) x install

uninstall:
	$(CARGO) x uninstall
