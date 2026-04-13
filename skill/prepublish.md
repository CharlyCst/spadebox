---
name: prepublish
description: Step-by-step checklist for publishing a new version of Spadebox to crates.io (Rust) and NPM (JS bindings).
---

# Publishing Spadebox

This skill covers publishing both the Rust crates and the JS bindings.

| Package | Published by | How |
|---|---|---|
| Rust crates (`spadebox-core`, `spadebox-mcp`) | User, manually | `cargo publish` |
| JS bindings (`@spadebox/spadebox`) | CI | Triggered automatically |

---

## Rust Crates

Rust crates are published manually by the user.

### Checklist

1. **Bump the version** in the relevant `Cargo.toml` files.

2. **Dry-run** to catch any issues before publishing:
   ```
   cargo publish --workspace --dry-run
   ```

3. If the dry-run passes, **publish**:
   ```
   cargo publish --workspace
   ```

---

## JS Bindings

The JS bindings are published through CI. The CI handles building native
binaries for each target platform and publishing both the platform packages and
the main package.

### Important: Manual Version Bump Required

Before triggering the CI, two version fields in `js/package.json` must be
updated **manually** — there is no automation for this step:

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

2. **Update `"optionalDependencies"`** in `js/package.json` to match the new
   version (see above).

3. **Trigger the CI** — the CI will build each platform binary and publish all
   packages to NPM.

### Verification

After the CI completes, verify the install works end-to-end in a fresh project:

```sh
mkdir /tmp/spadebox-verify && cd /tmp/spadebox-verify
npm init -y
npm install @spadebox/spadebox@<new-version>
node -e "const sb = require('@spadebox/spadebox'); console.log(Object.keys(sb));"
```

This should print `[ 'SpadeBox' ]` without errors.
