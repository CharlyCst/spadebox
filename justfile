build:
    cargo build
    cd js && deno task build:debug

build-release:
    cargo build --release
    cd js && deno task build

test:
    # First, we rebuild the bindings
    @just build
    # Then, we run tests in all languages
    cargo test
    cd js && deno check && deno task test
