# Release Process

This document describes how to release peat-btle to crates.io (Rust) and Maven Central (Android).

## Prerequisites

### Rust (crates.io)
- Cargo installed with `cargo login` configured
- Publish access to the `peat-btle` crate

### Android (Maven Central)
- GPG key for signing artifacts (current: `B4D250D0`)
- Sonatype Central Portal credentials
- Credentials configured in `~/.gradle/gradle.properties`:

```properties
# Maven Central (Sonatype) publishing
sonatypeUsername=YOUR_USERNAME
sonatypePassword=YOUR_PASSWORD

# GPG Signing
signing.gnupg.keyName=B4D250D0
signing.gnupg.passphrase=
```

## Version Bump

Update version in both:
1. `Cargo.toml` - `version = "X.Y.Z"`
2. `android/build.gradle.kts` - `version = "X.Y.Z"`

## Publishing to crates.io (Rust)

```bash
# Verify the package
cargo publish --dry-run

# Publish
cargo publish
```

## Publishing to Maven Central (Android AAR)

### 1. Build Native Libraries

```bash
cd android
./gradlew buildNativeLibs
```

This builds `libpeat_btle.so` for:
- `arm64-v8a` (modern Android devices)
- `armeabi-v7a` (older devices)

### 2. Test Locally (Optional)

```bash
# Publish to local Maven (~/.m2)
./gradlew publishLocal

# Or publish to project-local repo
./gradlew publishReleasePublicationToLocalRepository
```

### 3. Publish to Maven Central

```bash
./gradlew publishToMavenCentral
```

This will:
1. Build the AAR with native libraries
2. Generate sources JAR
3. Sign all artifacts with GPG
4. Create a bundle ZIP
5. Upload to Sonatype Central Portal with automatic publishing

### 4. Verify Publication

- Check status at: https://central.sonatype.com
- Search for artifact: https://search.maven.org/search?q=g:com.revolveteam

The artifact will be available as:
```gradle
implementation("com.revolveteam:peat-btle:X.Y.Z")
```

Note: Maven Central indexing can take 10-30 minutes after successful publication.

## GPG Key Management

### View Current Keys
```bash
gpg --list-keys --keyid-format SHORT kit@revolveteam.com
```

### Upload Key to Keyservers
Maven Central requires the signing key to be on public keyservers:
```bash
gpg --keyserver keys.openpgp.org --send-keys KEY_ID
gpg --keyserver keyserver.ubuntu.com --send-keys KEY_ID
gpg --keyserver pgp.mit.edu --send-keys KEY_ID
```

### Create New Signing Key (if needed)
```bash
gpg --batch --gen-key <<EOF
%no-protection
Key-Type: RSA
Key-Length: 4096
Subkey-Type: RSA
Subkey-Length: 4096
Name-Real: Your Name
Name-Email: your@email.com
Expire-Date: 2y
%commit
EOF
```

## CI/CD (GitHub Actions)

For automated releases, set these repository secrets:
- `SONATYPE_USERNAME`
- `SONATYPE_PASSWORD`
- `GPG_PRIVATE_KEY` (ASCII-armored key)
- `GPG_KEY_ID`

## Troubleshooting

### GPG Signing Timeout
Ensure `allow-loopback-pinentry` is in `~/.gnupg/gpg-agent.conf`:
```bash
echo "allow-loopback-pinentry" >> ~/.gnupg/gpg-agent.conf
gpg-connect-agent reloadagent /bye
```

### Maven Central Validation Failed - Invalid Signature
The GPG key must be uploaded to keyservers Maven Central checks. Upload to all three:
- keys.openpgp.org
- keyserver.ubuntu.com
- pgp.mit.edu

### Native Libraries Missing
Run `./gradlew buildNativeLibs` before publishing. Requires:
- Android NDK installed
- Rust Android targets: `rustup target add aarch64-linux-android armv7-linux-androideabi`
