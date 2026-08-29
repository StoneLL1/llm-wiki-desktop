# ADR 0001: Stop native capability installation until runner confinement is proven

- Status: Accepted — release stop
- Date: 2026-08-30
- Decision owners: desktop runtime and release engineering
- Related plan: Batch 5 of `2026-08-29-import-release-readiness-and-capability-management-implementation-plan.md`

## Context

Capability archives are authenticated before installation, but authenticity is not a runtime permission boundary. The current native pack engine starts a downloaded executable as an ordinary child process. Clearing inherited environment variables and terminating the process tree limit accidental leakage and process lifetime; they do not prevent the runner from reading the project, writing outside its staging directory, opening the network, or starting another executable.

Batch 5 requires an up-front feasibility gate for Windows x64, macOS arm64, macOS x64, and Ubuntu x64. Every target must prove filesystem, network, and child-process restrictions in a real packaged application. If one target cannot do so, the plan requires the batch to stop before multi-route activation or the Batch 6 mutation UI is implemented.

## Feasibility result

| Release target | Supported mechanisms considered | Gap against the release contract | Result |
| --- | --- | --- | --- |
| Windows x64 | AppContainer/restricted token, dedicated ACLs, Job Objects, child-process policy | The application has no production adapter that creates a confined token/container, constructs item-scoped ACLs, applies process-creation restrictions, and proves teardown in a packaged build. Microsoft's newer `CreateProcessInSandbox` API is explicitly experimental and Windows 11-only, so it cannot be the release baseline. | Not proven |
| macOS arm64 | App Sandbox, sandboxed helpers/XPC, inherited helper entitlements | The current downloadable runner is not an embedded, signed sandboxed helper and the desktop bundle does not define the entitlement/helper topology required to prove item-scoped access. App Sandbox network access is entitlement-based and does not by itself provide the declared endpoint policy required by the plan. | Not proven |
| macOS x64 | Same as macOS arm64 | Same architecture and distribution gap; CPU architecture does not remove it. | Not proven |
| Ubuntu x64 | User/mount/network namespaces, seccomp, Landlock, bubblewrap | No production adapter or packaged dependency/kernel-ABI policy exists. Landlock alone is insufficient for the complete contract: filesystem rules depend on kernel ABI, network rules are port-based, and child process/system-call restrictions require additional controls. A no-support fail-closed path has not been packaged and tested. | Not proven |

The gaps above are repository-specific conclusions from comparing the current runner path with the platform contracts. They are not claims that the operating systems lack sandboxing mechanisms.

Primary platform references:

- Microsoft: [Create Process in Sandbox](https://learn.microsoft.com/en-us/windows/win32/secauthz/createprocessinsandbox), [Implementing an AppContainer](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer), and [UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute).
- Apple: [Embedding a helper tool in a sandboxed app](https://developer.apple.com/documentation/xcode/embedding-a-helper-tool-in-a-sandboxed-app), [App Sandbox entitlement keys](https://developer.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html), and [Diagnosing App Sandbox violations](https://developer.apple.com/documentation/security/discovering-and-diagnosing-app-sandbox-violations).
- Linux kernel: [Landlock userspace API](https://cdn.kernel.org/doc/html/latest/userspace-api/landlock.html).

## Decision

1. Stop Batch 5 at the mandatory feasibility gate. Do not implement multi-route activation, platform confinement adapters, or malicious fixtures on top of the current runner protocol.
2. Keep capability inventory and already-installed healthy runtime facts readable, but fail every confirmed new install/resume mutation with the stable backend code `APP_CAPABILITY_CONFINEMENT_UNAVAILABLE` before catalog lookup, task creation, continuation persistence, download, or installer write.
3. Expose `installAllowed: false` and `installBlockedReasonCode` in the app-global capability snapshot when a signed catalog entry exists but confinement is unavailable. Import requirement and ASR planning surfaces likewise remain non-installable.
4. Keep Batch 6 mutation actions blocked. A future UI may consume the read-only snapshot, but it must not expose install/update/resume controls until this ADR is superseded.
5. Preserve the previous healthy installed version. This stop does not delete installed archives, rewrite activation state, alter project content, or retrofit confinement onto an already-installed runner. Existing routes may still execute under the prior unsandboxed runtime contract; they are not release evidence and the distributable product remains No-Go.

## Consequences

- Release status remains **No-Go**. Source-level tests cannot replace the four-target packaged malicious-runner evidence.
- Batch 5's multi-route atomic activation acceptance criteria remain intentionally unimplemented because the plan forbids continuing past a failed feasibility gate.
- Capability installation is unavailable even if a catalog entry is signed and otherwise valid. This is a deliberate fail-closed behavior, not a transient download error.
- The additive DTO fields allow the management UI and Import to explain the block without inferring policy from catalog or task state.
- The stop prevents new installation mutations; it does not claim that an already-installed native pack has become sandboxed or read-only at execution time.

## Conditions to supersede this decision

A replacement design must first freeze a protocol in which the runner never receives the project root and only sees an item-scoped invocation root with explicit read-only input/runtime mounts plus separate output/temp. It must also define default-deny network and child-process behavior. Two plausible directions are:

- fixed, app-shipped and platform-signed broker/helper processes that own all filesystem and network access while downloaded packs execute in a smaller runtime; or
- a non-native capability runtime such as WASI/component execution with brokered host calls, if it can support every committed product route.

Before installation can be re-enabled, Windows x64, macOS arm64, macOS x64, and Ubuntu x64 packaged builds must all pass the Batch 5 malicious matrix, including project/sibling access, traversal/link/TOCTOU attempts, undeclared network, private/metadata endpoints, shell launch, orphan children, stable refusal errors, invocation-root cleanup, and unchanged project byte inventory.
