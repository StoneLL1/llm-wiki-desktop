# LLM Wiki Desktop updater patch

This directory vendors the published `tauri-plugin-updater` crate at version `2.9.0` (crates.io checksum `27cbc31740f4d507712550694749572ec0e43bdd66992db7599b89fbfd6b167b`). The upstream Apache-2.0 and MIT license files are preserved.

The local patch adds `UpdaterBuilder::max_manifest_bytes` and streams updater manifest response bodies through that hard limit before JSON deserialization. Upstream 2.9.0 otherwise calls `Response::json()` without a response-size hook, which cannot satisfy this application's Batch 4A manifest boundary.

When upgrading or removing the vendor pin:

1. Confirm the candidate upstream API enforces an equivalent transport-level limit for the response that creates `Update`.
2. Keep the `updater_configuration_uses_the_frozen_public_trust_anchor` contract green.
3. Re-run the full repository gate and both updater review passes.
