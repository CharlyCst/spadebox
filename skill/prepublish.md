---
name: prepublish
description: Step-by-step checklist for publishing a new version of Spadebox to crates.io (Rust), NPM (JS bindings), and PyPI (Python bindings).
---

# Publishing Spadebox

This skill covers publishing the Rust crates, JS bindings, and Python bindings.

| Package                                                       | Published by   | How                                 |
| ------------------------------------------------------------- | -------------- | ----------------------------------- |
| Rust crates (`spadebox-core`, `spadebox-mcp`, `spadebox-cli`) | User, manually | `cargo publish`                     |
| JS bindings (`@spadebox/spadebox`)                            | CI             | Triggered automatically on tag push |
| Python bindings (`spadebox`)                                  | CI             | Triggered automatically on tag push |

---

## Pre-flight

Run lints abd tests before doing anything else:

```
just lint
just test
```

Fix any issues before proceeding with version bumps or publishing.

---

## Changelog

Before bumping any version, add a new entry to `CHANGELOG.md` for the release:

1. Add a `## [<new-version>] - <date>` section at the top (below the header).
2. Populate it from the git log since the previous release:
   ```
   git log <prev-tag>..HEAD --oneline
   ```
   Group changes under `### Added`, `### Changed`, or `### Fixed` as appropriate. Skip internal chores (formatting,
   todos, CI tweaks).
3. Commit the changelog update together with the version bumps.

---

## Rust Crates

Rust crates are published manually by the user.

### Checklist

1. **Bump the version** in the relevant `Cargo.toml` files:
   - `crates/spadebox-core/Cargo.toml`
   - `crates/spadebox-mcp/Cargo.toml` — also update the `spadebox-core` dependency version
   - `crates/spadebox-cli/Cargo.toml` — also update the `spadebox-core` dependency version

2. **Dry-run** to catch any issues before publishing:
   ```
   cargo publish --workspace --dry-run --allow-dirty
   ```

3. The user will have to run (never run it yourself):
   ```
   cargo publish --workspace
   ```

---

## JS Bindings

The JS bindings are published through CI. The CI handles building native binaries for each target platform and
publishing both the platform packages and the main package.

### Important: Manual Version Bump Required

Before triggering the CI, two version fields in `js/package.json` must be updated **manually** — there is no automation
for this step:

1. The top-level `"version"` field.
2. All three versions in `"optionalDependencies"`, which must match `"version"`:

```json
"optionalDependencies": {
  "@spadebox/spadebox-linux-x64-gnu": "<new-version>",
  "@spadebox/spadebox-linux-arm64-gnu": "<new-version>",
  "@spadebox/spadebox-darwin-arm64": "<new-version>"
}
```

These correspond to the targets listed in `napi.targets`.

### Checklist

1. **Bump `"version"`** in `js/package.json`.

2. **Update `"optionalDependencies"`** in `js/package.json` to match the new version (see above).

3. **Trigger the CI** — the CI will build each platform binary and publish all packages to NPM.

### Verification

After the CI completes, verify the install works end-to-end in a fresh project:

```sh
mkdir /tmp/spadebox-verify && cd /tmp/spadebox-verify
npm init -y
npm install @spadebox/spadebox@<new-version>
node -e "const sb = require('@spadebox/spadebox'); console.log(Object.keys(sb));"
```

This should print `[ 'SpadeBox' ]` without errors.

---

## Python Bindings

The Python bindings are published through CI via maturin. The CI builds wheels for each target platform and publishes
them to PyPI.

### Important: Manual Version Bump Required

Before triggering the CI, the version must be updated **manually** in two files:

1. `python/pyproject.toml` — the `version` field under `[project]`.
2. `python/Cargo.toml` — the `version` field under `[package]`.

Both must be kept in sync.

### Checklist

1. **Bump `version`** in `python/pyproject.toml`.

2. **Bump `version`** in `python/Cargo.toml` to match.

3. **Trigger the CI** — the CI will build wheels for all targets and publish to PyPI.

### Verification

After the CI completes, verify the install works end-to-end:

```sh
pip install spadebox==<new-version>
python -c "import spadebox; print(dir(spadebox))"
```
