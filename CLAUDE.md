# Claude Code Project Guide - eche-btle

## Project Overview

eche-btle is a Bluetooth Low Energy mesh networking library with CRDT-based state synchronization. It's designed to be **fully self-sufficient** - complete security, identity, and mesh management without external dependencies.

## Radicle Workflow

This project uses [Radicle](https://radicle.xyz) for decentralized code collaboration.

**Repository ID**: `rad:z458mp9Um3AYNQQFMdHaNEUtmiohq`
**Web UI**: https://app.radicle.xyz/nodes/seed.radicle.garden/rad:z458mp9Um3AYNQQFMdHaNEUtmiohq

### Quick Reference

```bash
# Sync before starting work
rad sync --fetch

# Check for open patches
rad patch list

# Create a patch (from feature branch)
git push rad HEAD:refs/patches -o patch.message="feat: My change"

# Merge a patch (as maintainer)
rad patch checkout <id>        # or: git fetch <remote> && git merge <branch>
git checkout main
git merge patch/<id>
git push rad main
```

### Creating Patches

```bash
# 1. Start from current main
rad sync --fetch
git checkout main
git pull rad main

# 2. Create feature branch
git checkout -b feature/my-feature

# 3. Make changes and commit
git add .
git commit -m "feat: description"

# 4. Create patch
git push rad HEAD:refs/patches -o patch.message="feat: My change"

# 5. Verify
rad patch list
rad patch show <patch-id>
```

### Reviewing and Merging Patches

**Check patch status:**
```bash
rad patch list                    # List open patches
rad patch show <id>               # Show details (look for ✓ = CI passed)
rad patch diff <id>               # View changes
```

**Merge a patch (when CI passes):**

If patch is based on current main (ahead X, behind 0):
```bash
# Fast-forward merge
rad patch checkout <id>
git checkout main
git merge patch/<id>
git push rad main
# Patch auto-marked as merged ✓
```

If patch is behind main (ahead X, behind Y):
```bash
# Option 1: Ask author to rebase and update their patch

# Option 2: Cherry-pick (if simple)
git fetch <author-remote>
git cherry-pick <commit-hash>
git push rad main
rad patch comment <id> --message "Cherry-picked as <new-hash>"
rad patch archive <id>
```

### When Patches Fall Behind Main

If your patch shows "behind N" commits:

```bash
# 1. Rebase your branch onto current main
git fetch rad
git checkout feature/my-feature
git rebase rad/main

# 2. Force-push to update the patch
git push rad HEAD:refs/patches --force
# This creates a new revision of the same patch
```

### Adding Remotes for Other Contributors

To review patches from other contributors:

```bash
# Get their DID from: rad patch show <id>
rad remote add <did> --name <alias>
git fetch <alias>

# Their patch branch will be at:
# <alias>/patches/<patch-id>
```

### Seed Sync and Web UI

Radicle is decentralized - changes propagate through seeds. Sometimes the web UI lags.

**Force sync:**
```bash
rad sync --announce              # Push your changes to seeds
rad sync --fetch                 # Pull latest from seeds
```

**Check seed status:**
```bash
# Verify your commit is on the seed
curl -s "https://seed.radicle.garden/api/v1/repos/rad:z458mp9Um3AYNQQFMdHaNEUtmiohq" | jq '.payloads["xyz.radicle.project"].meta.head'
```

**If web UI shows stale data:**
- Try a different node: `rosa.radicle.xyz`, `seed.radicle.garden`, `pine.radicle.garden`
- Wait 1-2 minutes for propagation
- Hard refresh browser (Ctrl+Shift+R)

### Issue Management

```bash
rad issue list                                        # List open issues
rad issue open --title "Title" --description "Desc"   # Create issue
rad issue comment <id> --message "Comment"            # Add comment
rad issue close <id>                                  # Close issue
```

## CI Status

GOA (GitOps Agent) runs CI automatically on patches.

### Check CI Status

```bash
# View patch with CI review status
rad patch show <patch-id>

# Check if GOA is running
ps aux | grep goa

# View GOA process (should show radicle watcher)
# Example: goa radicle --seed-url https://seed.radicle.garden --rid rad:z458mp9Um3AYNQQFMdHaNEUtmiohq ...

# Check patch sync to seed
curl -s "https://iris.radicle.xyz/api/v1/repos/rad:z458mp9Um3AYNQQFMdHaNEUtmiohq/patches" | jq '.[].title'
```

### CI Pipeline

The `.goa` script runs on every patch update:
1. **Authorization** - Delegate patches run automatically; community patches need "ok-to-test" comment
2. **Format** - `cargo fmt --check`
3. **Clippy** - `cargo clippy -- -D warnings`
4. **Tests** - `cargo test`
5. **Examples** - `cargo check --examples`

Results are posted as patch review (accept/reject).

### Manual CI Run

```bash
# Before pushing, run locally:
cargo fmt --check
cargo clippy --features linux -- -D warnings
cargo test --features linux
cargo check --examples
```

### GOA Configuration

GOA watches `seed.radicle.garden` with 60-second polling. Config in `.goa` script.

To restart GOA (if needed):
```bash
# Find and kill existing
pkill -f "goa radicle"

# Start fresh (from project root)
goa radicle \
  --seed-url https://seed.radicle.garden \
  --rid rad:z458mp9Um3AYNQQFMdHaNEUtmiohq \
  --command "bash .goa" \
  --watch-patches \
  --delay 60 \
  --timeout 600 \
  --local-path /home/kit/Code/revolve/eche-btle &
```

## Build Commands

```bash
# Rust library (Linux)
cargo build --features linux

# Rust library (all features for docs)
cargo doc --features linux

# Android AAR
cd android && ./gradlew assembleRelease

# Run tests
cargo test --features linux
```

## Release Process

See `RELEASE.md` for full details:

```bash
# 1. Update version in Cargo.toml
# 2. Update CHANGELOG.md
# 3. Commit and tag
git commit -am "chore: Bump version to X.Y.Z"
git tag vX.Y.Z

# 4. Publish to crates.io
cargo publish

# 5. Publish to Maven Central
cd android && ./gradlew publishToMavenCentral --no-configuration-cache

# 6. Push to Radicle
git push rad main --tags
```

## Architecture Notes

- **Self-sufficient**: eche-btle works standalone or as transport for HIVE framework
- **CRDT sync**: Counter, Emergency, Chat CRDTs for mesh state
- **Multi-platform**: Linux (BlueZ), Android, iOS, macOS, Windows, ESP32
- **Security**: ChaCha20-Poly1305 encryption, X25519 key exchange, Ed25519 identity

## Key Files

| Path | Purpose |
|------|---------|
| `src/lib.rs` | Public API exports |
| `src/eche_mesh.rs` | Core mesh logic |
| `src/sync/crdt.rs` | CRDT implementations |
| `src/security/` | Encryption, key management |
| `android/` | Android/Kotlin bindings |
| `docs/adr/` | Architecture Decision Records |

## Security Implementation Status

| Issue | Title | Status |
|-------|-------|--------|
| 8ba5742 | Identity Binding | ✓ Done (v0.1.0-rc.1) |
| 1cfc6ac | Mesh Genesis | ✓ Done (v0.1.0-rc.1) |
| ce8b9c7 | Encrypted Advertisements | ✓ Done (v0.1.0-rc.2) |
| fabeeec | Credential Persistence | ✓ Done (v0.1.0-rc.2) |
| eafb1f2 | Trust Hierarchy | Open |
| fcefa91 | Membership Control | Open |
| edc87d7 | Node Provisioning | Open |
| d245dcb | Key Rotation | Open |
