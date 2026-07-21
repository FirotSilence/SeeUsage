# Security policy

## Reporting a vulnerability

Do not open a public issue for a vulnerability that could expose credentials or
account information. Use the repository's private GitHub Security Advisory
flow after the public repository is created.

SeeUsage should never read Codex or MiniMax credential files. Reports showing
credential access, command injection, unsafe executable resolution, or raw
account responses written to disk are treated as high priority.

SeeUsage detects broadly writable executable paths and requires one-time,
provider-specific consent before using them. Consent is pinned to the canonical
executable path and can be revoked from the tray menu. Approving such a path
accepts the risk that another local user could later replace the CLI at the same
path. Checks cover the executable and its immediate containing directory; they
are a substitution warning, not code signing or file-identity pinning. The safe
choice remains a private per-user installation.

SeeUsage isolates provider environments, never serializes raw provider errors,
and contains spawned CLI process trees. Authentication must remain in the
official provider CLIs; API keys must never be added to SeeUsage configuration
or command-line arguments.

Short notes, reminder labels, and reminder schedules are user-authored local
data. They are stored in `settings.json`, never inserted into provider commands,
and never sent to Codex or MiniMax. They use the same per-user data directory
permissions as the usage cache, but are not separately encrypted; do not place
passwords or API keys in them.

The Windows liquid-glass effect samples the small desktop area directly behind
the HUD. The backdrop buffer is memory-only and must never be logged,
serialized, attached to diagnostics, or sent to either provider.

CLI search paths are permission-filtered before they reach a provider process.
The selected executable directory is included only after it is trusted or the
user explicitly approves it. Non-interactive commands have no inherited
standard input.

On Windows, `scripts/install-windows.ps1` installs the application under the
current user's local application directory with a protected ACL. This is the
recommended location before enabling launch at login.

Release packaging scripts remap build-machine source paths and produce a SHA-256
file next to each archive. A checksum detects accidental corruption only when it
comes from a trusted channel; the scripts do not sign or notarize binaries.
