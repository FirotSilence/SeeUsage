# SeeUsage

[简体中文](README.zh-CN.md)

SeeUsage is a tiny native meter for Codex and MiniMax usage quotas. It has no
webview, database, analytics, or bundled account credentials.

![SeeUsage Windows HUD live preview](assets/seeusage-preview.png)

## What it does

- Keeps a compact, draggable liquid-glass quota meter above the Windows
  taskbar.
- Shows notes and near-term reminders in an adaptive multi-column layout.
- Supports delayed, interval, daily, weekly, monthly, solar-date, and lunar-date
  reminders.
- Shows quota summaries in the Windows system tray or macOS menu bar.
- Shows or hides Codex and MiniMax independently from the **Display** menu.
- Tunes liquid-glass opacity, blur, tint, refraction, highlights, and motion
  with a live HUD preview.
- Switches immediately between the system language, English, Simplified or
  Traditional Chinese, Japanese, Korean, Spanish, French, and German.
- Refreshes every three minutes and on demand.
- Keeps the last successful normalized snapshot when a provider is offline.
- Changes the tray ring from green to amber to red as quota gets low.
- Can launch at login using native per-user operating-system facilities.
- Exposes `SeeUsage --once` for diagnostics and scripting.

The Windows desktop meter is pinned above other windows by default, does not
take focus, and stays out of the taskbar and `Alt+Tab`. Use **Keep HUD on top**
in the meter or tray context menu to change the persisted pin state. Drag
anywhere on the meter to move it; the position is remembered. Right-click the
meter or tray icon to add and delete notes and reminders, hide the meter, or
reset its position. Open **Liquid glass settings...** from the same menu to
adjust the six persisted material controls or restore their defaults. The
meter grows only as needed, distributes larger
collections across columns, and marks an imminent reminder in amber. Notes
stay above reminders, while reminders are ordered from most urgent to least
urgent. The meter shows the two most urgent reminders by default; use the
disclosure arrow below them to expand or collapse the remaining reminders.
Reminders more than two days away are hidden from the meter by default; toggle
**Show reminders beyond 2 days** in the right-click reminder menu to show them.

Reminder choices include delays or repeating intervals in minutes, hours,
days, and weeks; a fixed local date and time; daily, weekdays, weekends, one
weekday, or selected weekdays; a day of each month, the last day of each
month, and annual fixed Gregorian or Chinese lunar dates (including leap lunar
months). A due reminder stays highlighted and alternates
between two attention states instead of opening a dialog. Click a due reminder
on the Windows meter to acknowledge it, or right-click it and choose **Mark
next occurrence complete** to skip the next occurrence. The same completion
action is available in the tray or menu-bar reminder menu. Overdue reminders
remain pending after a restart; recurring reminders advance only when completed
and keep their interval cadence without drift. Right-click an individual
fixed-interval reminder and toggle **Update interval anchor when clicked** below
the completion action to schedule its next occurrence from the confirmation
time. Calendar and clock-based schedules never show or use this option.

## Requirements

- Windows 10 or later on x86_64, or macOS 12 or later on Apple silicon.
- A signed-in Codex CLI available on `PATH`.
- An authenticated [MiniMax CLI](https://github.com/MiniMax-AI/cli) with
  `mmx quota show` support available on `PATH`.
- SeeUsage checks whether CLI executables can be replaced by broad local groups
  such as `Users` or `Authenticated Users` and asks before using such a path.

Either provider may be unavailable; SeeUsage continues showing the other one.
Codex data is read through the documented local
[Codex App Server](https://learn.chatgpt.com/docs/app-server) protocol. MiniMax
data is read from `mmx quota show --output json --quiet`.

## Build and run

Install the stable Rust toolchain, then run:

```console
cargo run --release
```

Print one normalized snapshot without starting the tray application:

```console
cargo run -- --once
```

Create a portable Windows package:

```powershell
./scripts/package-windows.ps1
```

Install the Windows binary into a private per-user directory and start it:

```powershell
./scripts/install-windows.ps1
```

The installer keeps the source tree where it is, copies only the small binary
to `%LOCALAPPDATA%\Programs\SeeUsage`, and removes inherited write access from
other local users on both the program and settings directories. The packaged
`Install-SeeUsage.ps1` performs the same step without requiring Rust.

Create a macOS application bundle and zip archive:

```bash
./scripts/package-macos.sh
```

## Configuration

SeeUsage intentionally has no heavy settings panel. Notes, reminders, and
their deletion actions live in the native context menu. Choose **Language**
there to switch the entire interface without restarting; the selection is
stored locally in `settings.json`. The **Display** submenu independently hides
Codex or MiniMax from the HUD, native menu, tooltip, title, and quota warning
color. This changes presentation only; background refreshes continue for both
providers. Codex is visible by default and MiniMax is hidden by default. These
environment variables cover portable and development
installations:

| Variable | Purpose |
| --- | --- |
| `SEEUSAGE_CODEX_BIN` | Override the Codex executable or script path. |
| `SEEUSAGE_MMX_BIN` | Override the `mmx` executable or script path. |
| `SEEUSAGE_REFRESH_SECONDS` | Refresh interval, with a 30-second minimum. |

The default refresh interval is 180 seconds.

Executable overrides receive the same path-permission checks as tools found on
`PATH`. When a CLI path is broadly writable, the tray application shows a
one-time native warning for that provider and path. Choosing **Yes** remembers
the path and allows future refreshes; choosing **No** keeps cached data for the
current session. Use **Forget CLI path approvals** in the tray menu to revoke
consent. `--once` never opens approval dialogs. PowerShell wrappers remain
unsupported. On Windows, install SeeUsage itself in a private per-user
directory before enabling launch at login.

## Privacy

SeeUsage is local-only. It delegates authentication and network requests to the
existing `codex` and `mmx` installations, never reads their credential files,
and never stores tokens or raw provider errors. Its cache contains only
normalized percentages, reset timestamps, model labels, and safe status codes.
User-authored notes and reminder labels are stored separately in
`settings.json`; they are never sent to either provider.

On Windows, the liquid-glass renderer samples only the small desktop area
directly beneath and around the HUD to blur and refract its backdrop. These
pixels remain in process memory, are never written to the cache or sent over
the network, and are discarded when replaced or when SeeUsage exits.

Each provider receives a minimal environment: Codex does not receive MiniMax
keys, MiniMax does not receive OpenAI keys, and unrelated secrets are removed.
The child `PATH` contains only permission-checked directories, and each CLI
runs from its own checked or explicitly approved directory. Approval stores
only the canonical executable path, never a key. Child processes run inside a
contained process group, standard input is disabled for non-interactive
commands, and captured output is bounded. Authentication remains owned by the
official CLIs; SeeUsage only reports `login required` when their existing
session is unavailable.

Local data locations:

- Windows: `%LOCALAPPDATA%\SeeUsage\state.json` and `settings.json`
- macOS: `~/Library/Application Support/SeeUsage/state.json` and
  `settings.json`

Quit SeeUsage and remove its data directory to erase cached usage, notes,
reminders, saved HUD position, and CLI path approvals. Uninstalling the binary
does not remove this user-authored data automatically.

## Development

```console
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

The source is split into small provider, formatting, cache, tray, and operating
system integration modules. Provider parsers use sanitized fixtures and do not
require live accounts during tests.

## Status

SeeUsage is an early preview. The Codex App Server schema is versioned with the
installed Codex release, so parsing is deliberately tolerant and should remain
covered by fixtures when upstream payloads evolve.

SeeUsage is independent from OpenAI and MiniMax. See [NOTICE](NOTICE) for the
full non-affiliation statement.

## License

[MIT](LICENSE)
