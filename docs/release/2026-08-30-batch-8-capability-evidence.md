# Batch 8 capability release evidence

Decision: **Public beta No-Go; Batch 8 source/release contracts complete, Batch 9 sealed packaged matrix Pending**

This record contains no private key, token, private URL, user path, or user content. It does not claim that a tag, GitHub Release, signed production archive, desktop installer, or clean-host journey was executed.

## Manifest-derived evidence

| Evidence | Expected | Batch 8 result |
| --- | --- | --- |
| Product definitions | all user-visible forms have built-in or published ownership | executable verifier |
| Published matrix | exactly one entry per definition × four targets | 11 × 4 = 44 for the current manifest; count derived at runtime |
| Staging / qualification | every published definition points to an implemented, existing entrypoint | executable release-plan preflight |
| Signed contract | archive inventory contains exact routes, formats, target, protocol, entrypoint, runtime and license | runtime rejects missing/drifted contract before registration or activation |
| Real corpus contract | each published extension maps to a small redistributable real fixture plus normal/masquerade/corrupt/cancel/boundary generation | `capabilities/qualification-corpus.json` |
| Network contract | public, login-wall, restricted, unknown fallback and endpoint-policy cases; X production sample separate from fixture | release qualification contract |
| Catalog/embed | distributable count and binary bytes are manifest-derived and same-run exact | preflight/verifier contract; packaged proof Pending Batch 9 |

## Source-level verification template

Record the exact commit and unedited output for:

```powershell
npm run test:capability-tools
npm run check:release-config
npm run check:import-source-media
npm run check:import-v2-cutover
npm run check
```

For a protected capability workflow run, additionally record each archive name, target, compressed/installed/model bytes, archive SHA-256, manifest SHA-256, signing key ID, SBOM/NOTICE/provenance digests, runner qualification result, and the exact merged catalog digest. Do not record the signing secret.

## Batch 9 pending evidence

- one immutable candidate tag/commit/run and anonymous reverse-download;
- four clean real hosts/VMs completing install, proactive capability install, restart reuse, Import install-and-continue, interruption/resume, tamper/rollback, representative format/platform routes, Source commit, and uninstall preservation;
- exact embedded catalog bytes from the same sealed run;
- final owner Go/No-Go and separate stable-publication approval.
