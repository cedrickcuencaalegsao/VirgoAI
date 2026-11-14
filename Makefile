.DEFAULT_GOAL := all
SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c

.PHONY: all init run build release typecheck fmt clean models icons add-icon dmg

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

# ---------------------------------------------------------------------------
# DMG packaging (macOS)
#   make dmg
#   make dmg VERSION=1.2.0 BUNDLE_ID=com.you.virgoai
# ---------------------------------------------------------------------------
APP_NAME   ?= VirgoAI
VERSION    ?= $(shell grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
DMG_NAME   ?= $(APP_NAME)-$(VERSION).dmg
DMG_PATH   ?= build
APP_BINARY ?= target/release/virgo-ai
ICON       ?= assets/icons/app-icon.png
BUNDLE_ID  ?= com.example.virgoai
VERSION    ?= 0.1.0

APP_DIR := $(DMG_PATH)/$(APP_NAME).app
STAGE   := $(DMG_PATH)/dmg-stage
EXE     := $(notdir $(APP_BINARY))

dmg: release
	@test -f "$(APP_BINARY)" || { echo "Binary not found: $(APP_BINARY)"; echo "Available:"; ls target/release | grep -v '\.d$$'; exit 1; }
	@echo "==> Building $(APP_NAME).app"
	@rm -rf "$(APP_DIR)" "$(STAGE)" "$(DMG_PATH)/$(DMG_NAME)"
	@mkdir -p "$(APP_DIR)/Contents/MacOS" "$(APP_DIR)/Contents/Resources" "$(STAGE)"
	@cp "$(APP_BINARY)" "$(APP_DIR)/Contents/MacOS/$(EXE)"
	@printf '%s\n' \
	  '<?xml version="1.0" encoding="UTF-8"?>' \
	  '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
	  '<plist version="1.0"><dict>' \
	  '<key>CFBundleName</key><string>$(APP_NAME)</string>' \
	  '<key>CFBundleDisplayName</key><string>$(APP_NAME)</string>' \
	  '<key>CFBundleIdentifier</key><string>$(BUNDLE_ID)</string>' \
	  '<key>CFBundleExecutable</key><string>$(EXE)</string>' \
	  '<key>CFBundleIconFile</key><string>AppIcon</string>' \
	  '<key>CFBundlePackageType</key><string>APPL</string>' \
	  '<key>CFBundleShortVersionString</key><string>$(VERSION)</string>' \
	  '<key>CFBundleVersion</key><string>$(VERSION)</string>' \
	  '<key>NSHighResolutionCapable</key><true/>' \
	  '</dict></plist>' > "$(APP_DIR)/Contents/Info.plist"
	@if [ -f "$(ICON)" ]; then \
	    echo "==> Generating app icon"; \
	    iconset="$(DMG_PATH)/AppIcon.iconset"; \
	    rm -rf "$$iconset"; mkdir -p "$$iconset"; \
	    for s in 16 32 128 256 512; do \
	        sips -z $$s $$s "$(ICON)" --out "$$iconset/icon_$${s}x$${s}.png" >/dev/null; \
	        sips -z $$((s*2)) $$((s*2)) "$(ICON)" --out "$$iconset/icon_$${s}x$${s}@2x.png" >/dev/null; \
	    done; \
	    iconutil -c icns "$$iconset" -o "$(APP_DIR)/Contents/Resources/AppIcon.icns"; \
	    rm -rf "$$iconset"; \
	else \
	    echo "Note: $(ICON) not found, building without an icon"; \
	fi
	@echo "==> Ad-hoc signing"
	@codesign --force --deep -s - "$(APP_DIR)"
	@echo "==> Creating DMG installer"
	@cp -R "$(APP_DIR)" "$(STAGE)/"
	@ln -s /Applications "$(STAGE)/Applications"
	@hdiutil create -volname "$(APP_NAME)" -srcfolder "$(STAGE)" -ov -format UDZO "$(DMG_PATH)/$(DMG_NAME)"
	@rm -rf "$(STAGE)"
	@echo "Done: $(DMG_PATH)/$(DMG_NAME)"