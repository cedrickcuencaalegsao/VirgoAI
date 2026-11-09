.DEFAULT_GOAL := all
SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c

.PHONY: all init run build release typecheck fmt clean models icons add-icon

# ---------------------------------------------------------------------------
# Project configuration (override on the CLI, e.g. `make run CARGO_ARGS=--release`)
# ---------------------------------------------------------------------------
CARGO_ARGS ?=

# Ollama models pulled by `make models`
MODELS ?= qwen3:8b qwen2.5-coder:7b

# Icons: SVGs live in assets/icons and are registered in src/icons.rs
ICON_DIR    := assets/icons
ICON_RS     := src/icons.rs
ICON_STROKE ?= 1.5
LUCIDE_URL  := https://raw.githubusercontent.com/lucide-icons/lucide/main/icons

# Icons the app itself uses (downloaded by `make init`)
DEFAULT_ICONS := folder-open folder file-text chevron-right chevron-down refresh-cw x \
	panel-right check plus message-square database sliders-horizontal copy send square \
	trash-2 paperclip

# ---------------------------------------------------------------------------
# Cargo shortcuts
# ---------------------------------------------------------------------------
all: build

run:
	cargo run $(CARGO_ARGS)

build:
	cargo build $(CARGO_ARGS)

release:
	cargo build --release $(CARGO_ARGS)

# Named `typecheck` (not `check`) so `make add-icon check` works: Lucide has a `check` icon.
typecheck:
	cargo check $(CARGO_ARGS)

fmt:
	cargo fmt

clean:
	cargo clean

# ---------------------------------------------------------------------------
# Project setup
#   make init     check the toolchain, download the Lucide icons, fetch crates
#   make models   pull the Ollama models used by default
# ---------------------------------------------------------------------------
init:
	@echo "==> Checking toolchain"
	@command -v cargo >/dev/null || { echo "Rust not found. Install it from https://rustup.rs"; exit 1; }
	@command -v curl >/dev/null || { echo "curl not found"; exit 1; }
	@if command -v ollama >/dev/null; then echo "Ollama: found"; else echo "Warning: Ollama not found (https://ollama.com)"; fi
	@echo "==> Preparing folders"
	@mkdir -p "$(ICON_DIR)"
	@echo "==> Downloading Lucide icons"
	@$(MAKE) --no-print-directory add-icon ICONS="$(DEFAULT_ICONS)"
	@echo "==> Fetching Rust dependencies"
	cargo fetch
	@echo ""
	@echo "Done. Next: 'make models' to pull the Ollama models, then 'make run'."

models:
	@command -v ollama >/dev/null || { echo "Ollama not found (https://ollama.com)"; exit 1; }
	@for m in $(MODELS); do echo "==> ollama pull $$m"; ollama pull "$$m"; done

# ---------------------------------------------------------------------------
# Lucide icons
#   make add-icon sun
#   make add-icon sun moon hard-drive
#   make add-icon ICONS=check          (needed only if a name clashes with a target)
#   make icons                         re-download every icon already in assets/icons
#
# Each icon is saved to assets/icons/<name>.svg and registered in src/icons.rs,
# after which you can call it as icons::<name_with_underscores>().
# Browse names at https://lucide.dev/icons
# ---------------------------------------------------------------------------
add-icon:
	@if [ -z "$(strip $(ICONS))" ]; then \
	    echo "Usage: make add-icon <lucide-icon-name> [more names...]"; \
	    echo "Example: make add-icon sun moon hard-drive"; \
	    exit 1; \
	fi
	@mkdir -p "$(ICON_DIR)"
	@touch "$(ICON_RS)"
	@failed=0; \
	for name in $(ICONS); do \
	    case "$$name" in *[!a-z0-9-]*) echo "Skipping invalid icon name: $$name"; failed=1; continue;; esac; \
	    file="$(ICON_DIR)/$$name.svg"; \
	    tmp="$$(mktemp)"; \
	    if curl -fsSL "$(LUCIDE_URL)/$$name.svg" -o "$$tmp"; then \
	        sed 's/stroke-width="2"/stroke-width="$(ICON_STROKE)"/g' "$$tmp" > "$$file"; \
	        fn=$$(echo "$$name" | tr '-' '_'); \
	        if grep -Eq "^[[:space:]]*icon_fn!\($$fn," "$(ICON_RS)"; then \
	            echo "Updated icon: $$name"; \
	        else \
	            printf 'icon_fn!(%s, "%s.svg");\n' "$$fn" "$$name" >> "$(ICON_RS)"; \
	            echo "Added icon:   $$name  ->  icons::$$fn()"; \
	        fi; \
	    else \
	        echo "Error: Lucide icon '$$name' not found (see https://lucide.dev/icons)"; \
	        failed=1; \
	    fi; \
	    rm -f "$$tmp"; \
	done; \
	exit $$failed

icons:
	@$(MAKE) --no-print-directory add-icon ICONS="$$(cd $(ICON_DIR) && ls *.svg | sed 's/\.svg$$//' | tr '\n' ' ')"

# Take icon names given on the command line (make add-icon sun moon) and turn
# them into empty targets so Make does not complain about unknown goals.
ifneq (,$(filter add-icon,$(MAKECMDGOALS)))
ICON_GOALS := $(filter-out add-icon,$(MAKECMDGOALS))
ICONS ?= $(ICON_GOALS)
ifneq (,$(ICON_GOALS))
.PHONY: $(ICON_GOALS)
$(ICON_GOALS):
	@:
endif
endif