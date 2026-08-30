# ADR 0002: Execute official capability packs as trusted application components

- Status: Accepted
- Date: 2026-08-30
- Decision owner: product owner
- Supersedes: [ADR 0001](0001-capability-runner-confinement-feasibility-stop.md)
- Related plan: Batch 5R and Batch 6 of `2026-08-29-import-release-readiness-and-capability-management-implementation-plan.md`

## Context

The original Batch 5 required a real packaged OS-level filesystem, network, and child-process confinement proof on Windows x64, macOS arm64, macOS x64, and Ubuntu x64 before capability installation or the Batch 6 mutation UI could proceed. The feasibility pass found no production adapter that satisfied that contract on all four targets, so ADR 0001 correctly stopped the batch and added a temporary fail-closed installation gate.

That contract made the first release depend on a cross-platform sandbox architecture even though the product only accepts capability packs published through the application's own official release chain. The product priority is now to deliver the complete capability-management and Import experience. Official packs are maintained, reviewed, signed, versioned, and distributed as part of the application product; they are not a third-party plugin market.

## Decision

1. Treat a capability pack from the fixed official catalog as a trusted application component after its target, product definition, archive identity, SHA-256, signature, manifest, protocol version, and declared route set are verified.
2. Execute the verified runner as an ordinary child process. Windows AppContainer/restricted tokens, macOS App Sandbox helpers, and Linux namespace/seccomp/Landlock/bubblewrap confinement are not first-release, Batch 6, or release prerequisites.
3. Do not claim that capability runners are sandboxed or prevented by the OS from reading arbitrary user files, using the network, or starting child processes. Confirmation and details surfaces describe the pack's purpose and runtime permission summary without implying enforcement that does not exist.
4. Keep the functional and reliability boundaries that make official-pack execution deterministic and recoverable:
   - fixed official catalog and trusted key;
   - hash, signature, manifest, target, route-set, protocol, archive-path, and output-path validation;
   - item-scoped invocation input/output/temp contract and no intentional project-root argument; authenticated platform routes may additionally receive an app-managed, capability-dedicated connector profile, never the user's ordinary browser or shell profile;
   - sanitized bounded environment, output and log limits, deadline, cancellation, terminal flush, and process-tree cleanup;
   - application-global single-flight installation and durable continuation;
   - all declared routes probed before one immutable runtime snapshot is published;
   - old healthy version retention, activation journal, rollback receipt, and restart recovery.
5. Keep arbitrary URLs, local archives, third-party catalogs/markets, custom signing roots, and user-PATH runtimes outside the capability-management feature. Opening any of those sources requires a separate future trust and permission decision.
6. Rebaseline the unfinished implementation as Batch 5R: remove the hardcoded `APP_CAPABILITY_CONFINEMENT_UNAVAILABLE` mutation stop and its derived non-installable presentation, restore installation for verified official packs, and complete multi-route atomic activation. Batch 6 may enable its complete install/resume/cancel/retry/update UI once those backend functional contracts pass; it does not wait for OS confinement evidence.

## Consequences

- ADR 0001 remains historical evidence but no longer blocks implementation.
- Batch 5R has operationally removed the temporary confinement error, restored the verified official-catalog install/resume path, and made all declared routes publish as one runtime snapshot. Frontend mutation surfaces must still use the backend coordinator and must not manufacture installability.
- Release remains **No-Go** until the remaining functional batches, real official pack assets, four-target packaged journeys, and sealed-candidate acceptance pass. OS sandbox evidence is no longer part of that No-Go decision.
- A compromised official signing/release chain or a defective official runner can act with the host permissions available to the application process. This is an accepted consequence of the functionality-first model and makes catalog/key/release provenance review essential.
- Existing project trust, writable authority, immutable source, Git checkpoint, continuation revalidation, and compatible-layout rules are unchanged. The global pack installation does not grant permission to write a project.

## Follow-up evidence

Batch 5R proved official-catalog installability, zero publication before all route probes succeed, all-route atomic visibility, old-version restoration with explicit receipt metadata on activation failure, fail-closed prepared/probed/activated restart recovery, disposable health-probe cwd plus bounded invocation-root execution, cancellation/process cleanup, and app-global single-flight behavior. Signed pack-relative runtime arguments are resolved before cwd changes. Batch 6 must now prove the complete no-project and in-Import management journeys against those backend facts.
