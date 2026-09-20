BINARY      := protui
CMD_PATH    := ./cmd/protui
BUILD_DIR   := bin
VERSION     ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
LDFLAGS     := -s -w -X main.version=$(VERSION)

GO          ?= go

.DEFAULT_GOAL := help

## help: list the available targets
.PHONY: help
help:
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/## //' | awk -F': ' '{printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

## build: compile the binary into bin/
.PHONY: build
build:
	@mkdir -p $(BUILD_DIR)
	$(GO) build -trimpath -ldflags '$(LDFLAGS)' -o $(BUILD_DIR)/$(BINARY) $(CMD_PATH)

## install: install the binary into GOBIN
.PHONY: install
install:
	$(GO) install -trimpath -ldflags '$(LDFLAGS)' $(CMD_PATH)

## run: build and launch the UI
.PHONY: run
run: build
	./$(BUILD_DIR)/$(BINARY)

## test: run the test suite
.PHONY: test
test:
	$(GO) test ./...

## test-race: run the test suite under the race detector
.PHONY: test-race
test-race:
	$(GO) test -race ./...

## cover: report test coverage per package
.PHONY: cover
cover:
	$(GO) test -coverprofile=coverage.out ./...
	$(GO) tool cover -func=coverage.out

## fmt: format Go sources with gofmt
.PHONY: fmt
fmt:
	$(GO) fmt ./...

## fmt-check: verify gofmt formatting without writing
# gofmt -l exits 0 whether or not it listed anything, so the listing itself has
# to be what fails.
.PHONY: fmt-check
fmt-check:
	@unformatted="$$(gofmt -l .)"; \
	if [ -n "$$unformatted" ]; then \
		echo "These files need gofmt (run 'make fmt'):"; \
		echo "$$unformatted"; \
		exit 1; \
	fi

## format: format Markdown, JSON and YAML with prettier
.PHONY: format
format:
	npx --yes prettier --write .

## format-check: verify prettier formatting without writing
.PHONY: format-check
format-check:
	npx --yes prettier --check .

## lint: vet Go sources, then check formatting of both
# fmt-check is listed explicitly rather than left to golangci-lint's gofmt
# formatter, which does not run when golangci-lint is not installed.
.PHONY: lint
lint: fmt-check vet lint-go format-check

.PHONY: vet
vet:
	$(GO) vet ./...

# golangci-lint is optional: skipped with a note rather than failing the build
# for anyone who has not installed it.
.PHONY: lint-go
lint-go:
	@if command -v golangci-lint >/dev/null 2>&1; then \
		golangci-lint run; \
	else \
		echo "golangci-lint not installed; skipping (see https://golangci-lint.run)"; \
	fi

## tidy: prune and verify go.mod
.PHONY: tidy
tidy:
	$(GO) mod tidy
	$(GO) mod verify

## check: everything CI runs
.PHONY: check
check: tidy lint test

## hooks: install the git pre-commit hook
# Git only reads core.hooksPath from repository config, so this one line cannot
# ship itself with the checkout.
.PHONY: hooks
hooks:
	git config core.hooksPath .githooks

## clean: remove build artefacts
.PHONY: clean
clean:
	rm -rf $(BUILD_DIR) coverage.out
