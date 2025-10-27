# Documented commands will appear in the help text.
#
# Derived from: https://github.com/contribsys/faktory/blob/4e7b8196a14c60f222be1f63bdcced2c1a750971/Makefile#L252-L253
.PHONY: help
help:
	@grep -E '^[/0-9a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

.PHONY: test
test: ## Run tests
	cargo t

.PHONY: images/redis
images/redis: ## Build Redis with Redis Cell module docker image
	docker build . -f redis.Dockerfile -t redis-cell:latest

.PHONY: images/valkey
images/valkey: ## Build Valkey with Redis Cell module docker image
	docker build . -f valkey.Dockerfile -t valkey-cell:latest

.PHONY: images
images: images/redis images/valkey ## Build images for both Redis and Valkey with Redis Cell module

.PHONY: check
check: ## Run fmt, clippy, and doc checks
	cargo fmt --check
	cargo clippy --all-features --all-targets
	cargo doc --no-deps --all-features

