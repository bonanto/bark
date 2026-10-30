# Bark - System Volume Fix

Windows keeps resetting the volume of **System Sounds** to 100%, which affects system sounds and notifications. This is a well-known issue on Windows 10 and 11. Bark fixes it by holding the volume at the level you choose.

Only manual changes are accepted: those made in Bark itself, or in **Windows Settings → Sound → Volume mixer → System sounds**. Windows' automatic resets are ignored.

Bark runs quietly in the system tray. Click its icon once to open it.

---

## Features

- Holds the System Sounds volume at your chosen level
- Accepts manual changes from Bark or the Windows Volume mixer, and ignores Windows' automatic resets
- Global hotkeys for adjusting the volume and toggling Quiet mode
- Lives in the system tray; a single click on the icon opens Bark
- Free and open source, with no account, ads, or telemetry

## Hotkeys

| Shortcut | Action |
|---|---|
| `Win` + `Alt` + `PgUp` | Increase the volume |
| `Win` + `Alt` + `PgDn` | Decrease the volume |
| `Win` + `Alt` + `Q` | Toggle Quiet mode |

## Installation

Bark is available in two editions. Both provide the same functionality.

### Microsoft Store

Search for **Bark - System Volume Fix** in the Microsoft Store and install it. The Store keeps the app up to date automatically.

### GitHub installer

Download the latest installer from the [Releases](https://github.com/bonanto/bark/releases) page and run it. Bark is added to your system tray with a single click.

## Usage

1. Start Bark from the Start menu, or let it launch with Windows.
2. Click the Bark icon in the system tray to open it. If the icon is hidden, click the **^** arrow in the taskbar to show hidden icons.
3. Choose the System Sounds volume you want Bark to hold.

You can also change the volume at any time in **Windows Settings → Sound → Volume mixer**. Bark treats that as a deliberate change and keeps the new level.

## Requirements

- Windows 10 (version 1809 or later) or Windows 11

## Privacy and network use

Bark does not collect, store, or share any personal data. The two editions differ only in whether they check for updates:

| Edition | Network access | Updates |
|---|---|---|
| Microsoft Store | None. Bark never connects to the internet. | Handled by the Microsoft Store |
| GitHub installer | One request to GitHub to check whether a newer release of this repository exists. | You are notified of a new release and download it from this repository |

No personal information, device identifiers, or usage data are sent by either edition. See the full [Privacy Policy](PRIVACY.md).

## Uninstall

Remove Bark from **Settings → Apps → Installed apps**.

## Support and feedback

Found a bug or have a suggestion? Please [open an issue](https://github.com/bonanto/bark/issues) and include your Windows version, the Bark version, and the steps to reproduce the problem.

## License

Bark is free software released under the **GNU General Public License v3.0**. See [LICENSE](LICENSE) for the full text.

---

**Version 1.0.1** · © Bonanto, 2026 · GPLv3
