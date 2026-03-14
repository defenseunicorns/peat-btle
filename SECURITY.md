# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in peat-btle, please report it responsibly. **Do not open a public GitHub issue for security vulnerabilities.**

### How to Report

You have two options:

1. **Email**: Send a detailed report to [security@defenseunicorns.com](mailto:security@defenseunicorns.com)
2. **GitHub Security Advisories**: Use the [private vulnerability reporting](https://github.com/defenseunicorns/peat-btle/security/advisories/new) feature on this repository

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Response Timeline

- **Acknowledgment**: Within 3 business days
- **Initial assessment**: Within 10 business days
- **Fix timeline**: Dependent on severity

### Disclosure Policy

- We will acknowledge reporters in the remediation PR (unless anonymity is requested)
- We follow coordinated disclosure practices
- We aim to release patches before public disclosure

## Supported Versions

| Version | Supported |
|---------|-----------|
| latest  | Yes       |

## Security-Relevant Areas

peat-btle is a BLE mesh transport library. The following areas are particularly security-sensitive:

- **BLE transport**: Bluetooth Low Energy advertisement, connection handling, and data framing
- **ChaCha20-Poly1305**: Authenticated encryption of all mesh traffic
- **Ed25519 identity**: Peer identity generation, signing, and verification
- **TOFU trust model**: Trust-on-first-use peer authentication and trust store management

### Known Limitations

- **No revocation mechanism yet**: Compromised keys cannot currently be revoked across the mesh. If a peer key is compromised, all other peers must manually remove it from their trust stores.
- **No key rotation yet**: Long-lived Ed25519 identity keys cannot be rotated in place. Key rotation requires re-establishing trust with all peers.

When integrating peat-btle, follow these practices:

- Protect Ed25519 private keys at rest
- Be aware of the TOFU trust model's limitations in adversarial environments
- Validate all BLE frame inputs before processing
- Keep dependencies up to date
