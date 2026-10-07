# Signed manifest fixture

`visualstudio-17-release.channel.json` is the unmodified public Microsoft VS 17
release channel fetched from <https://aka.ms/vs/17/release/channel> on 2026-10-08
(build 17.14.37710.0). Preserve its exact bytes: the signed content includes the
comma before the final signature member.

The deterministic Windows test verifies the actual RSA signature and rejects
modified signed-info, signature and signer keys. It does not assert current
certificate trust: production additionally requires Windows online revocation,
current validity, code-signing usage, Microsoft publisher/root and release
binding. Live acquisition workflows exercise those checks.
