.PHONY: fmt lint test ci doc

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings -W clippy::pedantic -A unused

test:
	cargo test

coverage:
	cargo tarpaulin --out Lcov

all: fmt lint test

doc:
	cargo doc

make cli:
	cargo run -- --game-step 1000 --explorer1 nomad