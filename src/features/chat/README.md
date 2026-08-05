# Chat Feature

Source/Wiki-grounded question answering, typed citations, sessions, and saved
query records live here. Source-only projects do not require compile. The
backend must enforce project trust before external AI/Agent/Skill execution;
pure Chat does not require Git, while saving or overwriting a query requires
writable access and the applicable Git/hash policy.

When the project filesystem is read-only, a permitted trusted Chat session is
ephemeral: keep messages in memory and explain that session history cannot be
persisted. Returning from trust or AI configuration preserves the draft and
must not send it automatically.
