# Security Policy

## Supported versions

The project is pre-1.0 and has not published a stable release. Security fixes
are applied to `master` and ship with the next release candidate; there are no
backport branches yet.

## Reporting a vulnerability

**Please do not report security problems through public GitHub issues.**

Use **GitHub's private vulnerability reporting** on this repository:
Security tab → "Report a vulnerability". This reaches the maintainer directly
and keeps the details private until a fix ships.

Include in your report:

- a description of the issue and its impact,
- reproduction steps or a proof of concept,
- affected version or commit SHA.

You will get an acknowledgment within 7 days. Please avoid public disclosure
until a fix is released; coordinated disclosure timelines are flexible for a
hobby-scale project, and credit is given unless you prefer anonymity.

## Scope

In scope:

- Anything in this repository: the Tauri/Rust backend, React frontend,
  release workflows, capability-pack tooling, and the update pipeline.
- Failure of the app's own security boundaries: project path confinement,
  symlink/reparse-point defenses, credential storage, capability signature
  verification, updater signature verification, Git-checkpoint gates.

Out of scope:

- Vulnerabilities in dependencies themselves (report upstream; a GitHub
  advisory is still appreciated so we can bump the dependency).
- AI provider behavior after the user sends content to their configured
  provider.
- Social engineering, physical access, or compromised developer machines.

## Handling of secrets

If you find a leaked credential in the repository, report it privately and do
not open an issue naming the value. The project's policy is that signing keys
live only in GitHub protected secrets or OS credential stores; any leak is
treated as requiring immediate rotation.
