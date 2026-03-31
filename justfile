build:
    cargo build
    cd js && yarn build:debug

build-release:
    cargo build --release
    cd js && yarn build

test:
    # First, we rebuild the bindings
    @just build
    # Then, we run tests in all languages
    cargo test
    cd js && yarn test
