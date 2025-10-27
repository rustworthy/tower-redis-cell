REDIS_VERSION=8.2.2
VALKEY_VERSION=9.0.0
CELL_VERSION=0.4.0

REDIS_CELL_IMAGE_NAME=ghcr.io/rustworthy/redis-cell
REDIS_CELL_IMAGE_TAG=${REDIS_VERSION}-${CELL_VERSION}

VALKEY_CELL_IMAGE_NAME=ghcr.io/rustworthy/valkey-cell
VALKEY_CELL_IMAGE_TAG=${VALKEY_VERSION}-${CELL_VERSION}

# Documented commands will appear in the help text.
#
# Derived from: https://github.com/contribsys/faktory/blob/4e7b8196a14c60f222be1f63bdcced2c1a750971/Makefile#L252-L253
.PHONY: help
help:
	@grep -E '^[/a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

.PHONY: check
check: ## Run fmt, clippy, and doc checks
	cargo fmt --check
	cargo clippy --all-features --all-targets
	cargo doc --no-deps --all-features

.PHONY: image/redis/build
images/redis/build:
	docker build . -f redis.Dockerfile -t ${REDIS_CELL_IMAGE_NAME}:${REDIS_CELL_IMAGE_TAG}

.PHONY: image/valkey/build
images/valkey/build:
	docker build . -f valkey.Dockerfile -t ${VALKEY_CELL_IMAGE_NAME}:${VALKEY_CELL_IMAGE_TAG}

.PHONY: images
images: images/redis/build images/valkey/build
