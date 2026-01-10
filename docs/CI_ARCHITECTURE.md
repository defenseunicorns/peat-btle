# CI Architecture for Radicle-hosted Projects

This document outlines the CI/CD strategy for hive-btle, a Radicle-hosted project requiring CI status on patches for distributed team review.

## Requirements

1. **Automatic CI triggers** on patch creation/update and main branch pushes
2. **CI status visible on Radicle patches** for team review workflow
3. **Self-hosted** - runs on our own infrastructure
4. **Reliable** - battle-tested components, not alpha software

## Architecture Decision

After evaluating Radicle's native CI broker (`cib` + `radicle-native-ci`), we found it unreliable and poorly documented. We're implementing a hybrid approach using proven tools.

## Option A: GOA + Radicle HTTP API (Recommended)

Leverage [GOA (GitOps Agent)](https://github.com/kitplummer/goa) to watch Radicle seed nodes via HTTP API, triggering local CI on changes.

```
┌─────────────────────────────────────────────────────────────┐
│               Radicle Seed Node (iris.radicle.xyz)          │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  HTTP API (:443)                                      │  │
│  │  GET /api/v1/repos/{rid}          → head, patch count │  │
│  │  GET /api/v1/repos/{rid}/patches  → patch list + oids │  │
│  │  GET /api/v1/repos/{rid}/commits  → commit history    │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ polls
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Local CI Node                            │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────┐ │
│  │ GOA         │───▶│ CI Script   │───▶│ Status Reporter │ │
│  │ (polling)   │    │ (cargo test)│    │ (rad patch)     │ │
│  └─────────────┘    └─────────────┘    └─────────────────┘ │
│         │                                       │          │
│         │ ENV vars:                             │          │
│         │ GOA_PATCH_ID                          ▼          │
│         │ GOA_COMMIT_OID            ┌─────────────────────┐│
│         │ GOA_BASE_COMMIT           │ rad patch comment   ││
│         └──────────────────────────▶│ (updates COB)       ││
│                                     └─────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

### Radicle HTTP API Endpoints

Discovered on `iris.radicle.xyz`:

```bash
# Get repo info (includes current head)
curl https://iris.radicle.xyz/api/v1/repos/rad:z3fF7wV6LXz915ND1nbHTfeY3Qcq7
# Returns: { "payloads": { "xyz.radicle.project": { "meta": { "head": "68bfe19..." }}}}

# Get patches (includes oid, base, state)
curl https://iris.radicle.xyz/api/v1/repos/rad:z3fF7wV6LXz915ND1nbHTfeY3Qcq7/patches
# Returns: [{ "id": "b664229...", "revisions": [{ "oid": "624db3d...", "base": "68bfe19..." }]}]

# Get commits
curl https://iris.radicle.xyz/api/v1/repos/rad:z3fF7wV6LXz915ND1nbHTfeY3Qcq7/commits
```

### GOA Enhancement Required

Current GOA watches git remotes. Enhancement needed:

1. **New `--radicle` mode** - Poll Radicle HTTP API instead of git
2. **Watch both head and patches** - Detect main branch changes AND new/updated patches
3. **Inject Radicle-specific ENV vars**:
   - `GOA_RADICLE_RID` - Repository ID
   - `GOA_PATCH_ID` - Patch ID (if triggered by patch)
   - `GOA_COMMIT_OID` - Commit to test
   - `GOA_BASE_COMMIT` - Base commit (for patches)
   - `GOA_PATCH_STATE` - open/merged/archived
4. **State tracking** - Remember last seen patch revisions to detect updates

### Advantages
- Watches the distributed network, not just local storage
- CI fires on *anyone's* changes synced to seed
- GOA is proven, simple, single binary
- No external forge (Gitea/Forgejo) needed
- Direct Radicle integration

### Implementation Steps

1. **GOA Enhancement** (in kitplummer/goa):
   - Add `--radicle-url` flag for HTTP API endpoint
   - Add `--radicle-rid` flag for repository ID
   - Implement polling logic for `/repos/{rid}` and `/repos/{rid}/patches`
   - Track state in local file (last head, patch revision timestamps)
   - Inject ENV vars on change detection

2. **CI Script** (`scripts/ci.sh`):
   ```bash
   #!/bin/bash
   set -e
   # Clone from local Radicle storage at the specific commit
   git clone ~/.radicle/storage/$GOA_RADICLE_RID /tmp/ci-$$
   cd /tmp/ci-$$
   git checkout $GOA_COMMIT_OID

   # Run CI
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   cargo check --examples
   ```

3. **Status Reporter** - Use `rad patch comment` to report results back

## Option B: Woodpecker + Forgejo Bridge

Full-featured CI with web UI, but more infrastructure.

```
┌─────────────┐     ┌─────────────┐     ┌─────────────────┐
│ Radicle     │────▶│ Forgejo     │────▶│ Woodpecker CI   │
│ (source)    │mirror│ (forge)     │webhook│ (server+agent) │
└─────────────┘     └─────────────┘     └────────┬────────┘
                                                  │
                    ┌─────────────────────────────▼────────┐
                    │ Status Reporter (Forgejo → Radicle)  │
                    └──────────────────────────────────────┘
```

### Components

1. **Forgejo** - Local git forge, mirrors from Radicle
2. **Woodpecker Server** - CI orchestrator with web UI
3. **Woodpecker Agent** - Executes pipelines in containers
4. **Sync Script** - `rad sync` → `git push forgejo`
5. **Status Bridge** - Woodpecker webhook → `rad patch` status

### Advantages
- Full CI/CD platform with web UI
- Container isolation for builds
- Multi-repo support
- Plugin ecosystem

### Disadvantages
- More infrastructure (Forgejo + Woodpecker + containers)
- Sync delay between Radicle and Forgejo
- Status bridge adds complexity

## Option C: Minimal Event Watcher

Simplest possible solution - a shell script daemon.

```bash
#!/bin/bash
# Watches rad node events, triggers CI on patch events
rad node events --json | while read -r event; do
  if echo "$event" | jq -e '.type == "patch-created" or .type == "patch-updated"'; then
    run_ci "$event"
  fi
done
```

### Advantages
- Zero dependencies beyond rad CLI
- Easy to understand and debug

### Disadvantages
- No persistence (misses events during restarts)
- No retry logic
- Shell script fragility

## Recommended Approach: Option A (GOA)

GOA provides the right balance of reliability and simplicity. Implementation plan:

### Phase 1: Basic CI Trigger
- [ ] Install GOA on CI node
- [ ] Configure to watch Radicle storage
- [ ] Create CI script matching current `.radicle/native.yaml`
- [ ] Test with manual commits

### Phase 2: Patch Status Reporting
- [ ] Research `rad patch comment` API for status updates
- [ ] Create status reporter script
- [ ] Integrate with CI script
- [ ] Test patch workflow end-to-end

### Phase 3: Production Hardening
- [ ] Systemd service for GOA
- [ ] Log aggregation
- [ ] Failure notifications (optional)
- [ ] Documentation

## Current CI Pipeline

From `.radicle/native.yaml`:

```yaml
shell: |
  set -e
  cargo fmt --check
  cargo clippy -- -D warnings
  cargo test
  cargo check --examples
```

## Open Questions

1. **Patch status API** - How does `rad` CLI support updating patch CI status? Need to investigate COB structure and `rad patch comment` capabilities.
2. **Multi-patch handling** - If multiple patches update simultaneously, how to handle concurrent CI runs? Options: queue, parallel workers, or serialize.
3. **Seed node selection** - Which seed to poll? `iris.radicle.xyz` is official, but could use any seeding the repo.

## GOA Enhancement Roadmap

Target: Add Radicle support to [github.com/kitplummer/goa](https://github.com/kitplummer/goa)

### Phase 1: Radicle Polling
- [ ] Add `radicle` subcommand (or `--radicle` flag to `spy`)
- [ ] Implement HTTP client for Radicle API
- [ ] Poll `/repos/{rid}` for head changes
- [ ] Poll `/repos/{rid}/patches` for patch changes
- [ ] State file for tracking last-seen revisions

### Phase 2: CI Integration
- [ ] Inject Radicle-specific ENV vars
- [ ] Support `.goa` file with Radicle config
- [ ] Add `--on-patch` and `--on-push` separate triggers

### Phase 3: Status Reporting (stretch)
- [ ] Optional status callback (shell command or webhook)
- [ ] Integration example with `rad patch comment`

## References

- [GOA - GitOps Agent](https://github.com/kitplummer/goa)
- [Woodpecker CI](https://woodpecker-ci.org/)
- [Forgejo](https://forgejo.org/)
- [Radicle CI Broker](https://lib.rs/crates/radicle-ci-broker) (for reference, not recommended)
