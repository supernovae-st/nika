# BUG: NIKA-135 — vault decrypt fails with UnknownCryptoError on provider set

**Date**: 2026-04-02
**Severity**: P1 — blocks `nika keys set` on VPS deployments
**Nika version**: v0.62.0
**Environment**: nk-jungo-vps (Ubuntu 22.04, Scaleway PLAY2-MICRO)
**Reporter**: Thibaut Melen

---

## Problem

`nika keys set <provider>` fails with:
```
✕ [NIKA-135] Config error: Failed to store key: vault crypto error: decrypt failed: UnknownCryptoError
```

This happens for ALL providers (tested mistral and openai). The vault file exists
(`~/.nika/secrets/vault.enc`, 334 bytes) but cannot be decrypted.

## Reproduction

```bash
nika@nk-jungo-vps:~$ nika keys set mistral
◇  Paste your mistral API key:
│  ●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●●
│
│  ✕ [NIKA-135] Config error: Failed to store key: vault crypto error: decrypt failed: UnknownCryptoError

nika@nk-jungo-vps:~$ nika keys set openai
◇  Paste your openai API key:
│  ●●●●●●●●●●●●...
│
│  ✕ [NIKA-135] Config error: Failed to store key: vault crypto error: decrypt failed: UnknownCryptoError
```

## Root Cause Analysis

### Vault state on disk
```
~/.nika/secrets/
├── vault.enc     334 B   # Encrypted vault (XChaCha20Poly1305)
├── vault.salt     16 B   # Argon2i salt for key derivation
├── vault.lock      0 B   # File lock
└── audit.jsonl   5.3 KB  # Access log
```

### Missing passphrase
```bash
$ env | grep VAULT
# (empty — NIKA_VAULT_PASSPHRASE is not set)
```

The vault was likely created during a previous `nika keys set` call where the passphrase
was either:
1. Set interactively and not persisted to `.env` or shell profile
2. Derived from a default that changed between versions
3. Set via a different user session (root vs nika)

### The decrypt flow
1. `nika keys set` → `vault.get_or_create()` → tries to decrypt `vault.enc`
2. Derives encryption key from `NIKA_VAULT_PASSPHRASE` via Argon2i (6 iterations + salt)
3. If passphrase is empty/wrong → Argon2i produces wrong key → XChaCha20Poly1305 decrypt fails
4. Error: `UnknownCryptoError` (generic crypto error, no detail on what failed)

### The real issue
The error message doesn't help the user understand what happened. There are several UX problems:

1. **No hint about passphrase**: The error says "decrypt failed" but doesn't mention
   `NIKA_VAULT_PASSPHRASE` or suggest setting it
2. **No recovery path**: If the passphrase is lost, there's no `nika vault reset` command
3. **Silent vault creation**: The first `nika keys set` creates the vault silently.
   If the passphrase changes (or was never set), all subsequent calls fail
4. **Empty passphrase accepted**: If `NIKA_VAULT_PASSPHRASE` is not set, nika uses
   an empty string (or a default) which varies by version/environment

## Expected Behavior

1. **Clear error message**: 
   ```
   ✕ [NIKA-135] Vault decrypt failed. 
     The vault passphrase doesn't match the one used when the vault was created.
     
     Options:
       • Set NIKA_VAULT_PASSPHRASE to the original passphrase
       • Run `nika vault reset` to delete the vault and start fresh
       • Use environment variables instead: export OPENAI_API_KEY=sk-...
   ```

2. **Recovery command**: `nika vault reset` should delete `vault.enc` + `vault.salt` and
   allow recreating with a new passphrase

3. **Vault status in doctor**: `nika doctor` should check if the vault is readable and
   warn if it's not

## Workaround

API keys work fine via environment variables — they bypass the vault entirely:

```bash
# In ~/.nika/.env (sourced before nika serve):
OPENAI_API_KEY=sk-...
XAI_API_KEY=xai-...
NIKA_SERVE_TOKEN=...
```

`nika serve` picks up env vars for all providers. `nika provider list` shows them as
unconfigured (it only checks the vault) but the workflows run fine.

### Alternative: delete vault and recreate

```bash
# Nuclear option — loses all stored keys
rm ~/.nika/secrets/vault.enc ~/.nika/secrets/vault.salt
export NIKA_VAULT_PASSPHRASE="my-new-passphrase"
nika keys set openai   # Creates fresh vault
```

## Affected Code

- `tools/nika-cli/src/provider.rs` — `provider_set()` function
- `tools/nika-engine/src/vault/` — `NikaVault::decrypt()`, `NikaVault::get_or_create()`
- Error type: `NikaError::ConfigError` with code NIKA-135

## Recommendations

1. **P0**: Improve error message to mention `NIKA_VAULT_PASSPHRASE` and recovery options
2. **P1**: Add `nika vault reset` command
3. **P1**: Add vault health check to `nika doctor`
4. **P2**: Consider using OS keychain (macOS Keychain, Linux Secret Service) as alternative
   to file-based vault for interactive use
5. **P2**: Log the passphrase source in debug mode ("using NIKA_VAULT_PASSPHRASE" vs
   "using default passphrase" vs "no passphrase set")
