build:
    cargo build --workspace --exclude spadebox-python
    cd js && deno task build:debug
    cd python && uv run maturin develop

build-release:
    cargo build --release --workspace --exclude spadebox-python
    cd js && deno task build
    cd python && uv run maturin build --release

test:
    # First, we rebuild the bindings
    @just build
    # Then, we run tests
    @just test-only

test-only:
    cargo test
    cd js && deno check && deno task test
    cd python && uv run pytest test

lint:
    cargo fmt -- --check
    cargo clippy
    cd js && deno lint
