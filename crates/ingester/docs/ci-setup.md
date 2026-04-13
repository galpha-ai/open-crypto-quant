# CI Setup for Private Repository Dependencies

This document describes how to set up CI access for private repository dependencies in the ingester project.

## Overview

The ingester project depends on the private `tx_sub` repository. To allow CI environments to access this dependency, we use SSH key authentication.

## Setup Process

### 1. Generate SSH Key Pair

```bash
ssh-keygen -t ed25519 -C "ci@popeyes"
```

This creates two files:
- `id_ed25519` (private key)
- `id_ed25519.pub` (public key)

### 2. Add Public Key as Deploy Key

1. Go to the `tx_sub` repository on GitHub
2. Navigate to Settings → Deploy keys
3. Click "Add deploy key"
4. Paste the contents of `id_ed25519.pub`
5. Give it a descriptive title (e.g., "CI Deploy Key")
6. Leave "Allow write access" unchecked (read-only access is sufficient)

### 3. Add Private Key as Repository Secret

1. Go to the `ingester` repository on GitHub
2. Navigate to Settings → Secrets and variables → Actions
3. Click "New repository secret"
4. Name: `CI_SSH_PRIVATE_KEY`
5. Value: Paste the contents of `id_ed25519` (the private key)

## GitHub Actions Configuration

In your workflow file, use the SSH key like this:

```yaml
- name: Setup SSH key
  uses: webfactory/ssh-agent@v0.7.0
  with:
    ssh-private-key: ${{ secrets.CI_SSH_PRIVATE_KEY }}

- name: Add GitHub to known hosts
  run: ssh-keyscan github.com >> ~/.ssh/known_hosts

- name: Build project
  run: cargo build
```

## Security Notes

- The private key should never be committed to the repository
- The deploy key only has read access to the `tx_sub` repository
- Consider using organization secrets if multiple repositories need the same access