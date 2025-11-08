REDIS_VERSION=8.2.2
VALKEY_VERSION=9.0.0
CELL_VERSION=0.4.0

# Documented commands will appear in the help text.
#
# Derived from: https://github.com/contribsys/faktory/blob/4e7b8196a14c60f222be1f63bdcced2c1a750971/Makefile#L252-L253
.PHONY: help
help:
	@grep -E '^[/0-9a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

.PHONY: doc
doc:
	RUSTDOCFLAGS='--cfg docsrs' cargo +nightly d --all-features --open -p tower-redis-cell

.PHONY: test
test: ## Run tests
	cargo t --all-features

.PHONY: test/doc
test/doc: ## Run doc tests
	cargo t --doc

.PHONY: images/redis
images/redis: ## Build Redis with Redis Cell module docker image
	docker build . -f redis.Dockerfile \
		-t redis-cell:latest \
		-t ghcr.io/rustworthy/redis-cell:latest \
		-t ghcr.io/rustworthy/redis-cell:${REDIS_VERSION}-${CELL_VERSION}

.PHONY: images/valkey
images/valkey: ## Build Valkey with Redis Cell module docker image
	docker build . -f valkey.Dockerfile \
		-t valkey-cell:latest \
		-t ghcr.io/rustworthy/valkey-cell:latest \
		-t ghcr.io/rustworthy/valkey-cell:${VALKEY_VERSION}-${CELL_VERSION}

.PHONY: images
images: images/redis images/valkey ## Build images for both Redis and Valkey with Redis Cell module

.PHONY: images/push
images/push:
	@echo ${REGISTRY_PASSWORD} | docker login ghcr.io -u rustworthy --password-stdin
	docker push ghcr.io/rustworthy/redis-cell:latest
	docker push ghcr.io/rustworthy/redis-cell:${REDIS_VERSION}-${CELL_VERSION}
	docker push ghcr.io/rustworthy/valkey-cell:latest
	docker push ghcr.io/rustworthy/valkey-cell:${VALKEY_VERSION}-${CELL_VERSION}

.PHONY: check
check: ## Run fmt, clippy, and doc checks
	cargo fmt --check
	cargo clippy --all-features --all-targets

