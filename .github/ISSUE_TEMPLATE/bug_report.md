name: Bug report
description: Something does not work as expected
labels: ["bug"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for taking the time to report. Please **never** paste API keys,
        credentials, or private knowledge-base content into this form —
        security issues go through [SECURITY.md](../blob/master/SECURITY.md).
  - type: input
    id: version
    attributes:
      label: App version or commit
      description: Version from Settings → About, or the commit SHA if running from source.
    validations:
      required: true
  - type: dropdown
    id: platform
    attributes:
      label: Platform
      options:
        - Windows x64
        - macOS (Apple Silicon)
        - macOS (Intel)
        - Linux x64
        - Running from source
    validations:
      required: true
  - type: textarea
    id: what-happened
    attributes:
      label: What happened?
      description: What you did, what you expected, and what actually happened. Error messages from the task drawer or status bar help a lot.
    validations:
      required: true
  - type: textarea
    id: knowledge-base
    attributes:
      label: Knowledge-base details (if relevant)
      description: Format (native / legacy LLM Wiki / Obsidian-compatible / read-only), approximate size, whether AI features were involved — no file contents please.
    validations:
      required: false
