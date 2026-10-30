# Privacy Policy

**Bark - System Volume Fix**
Publisher: Bonanto
Last updated: October 30, 2026

---

## 1. Summary

Bark does not collect, store, or share any personal data. It has no accounts, advertising, analytics, or telemetry.

| Question | Microsoft Store edition | GitHub edition |
|---|---|---|
| Collects personal information? | No | No |
| Collects usage data or telemetry? | No | No |
| Requires an account? | No | No |
| Shows ads? | No | No |
| Shares or sells data? | No | No |
| Connects to the internet? | **No** | **Only to check for a new release** (see section 5) |

## 2. About this policy

This policy describes how **Bark - System Volume Fix** ("Bark") handles information on Windows. Bark is published by Bonanto and is free, open-source software licensed under GPLv3. Its source code is available at https://github.com/bonanto/bark.

Bark is distributed in two editions: one through the Microsoft Store, and one as an installer from the GitHub Releases page. This policy covers both.

## 3. What Bark does

Windows frequently resets the System Sounds volume to 100%. Bark holds that volume at a level you choose. To do this, it uses standard Windows audio settings to read and set the volume of System Sounds. It does not record, capture, or listen to any audio.

Bark also registers global keyboard shortcuts (Win+Alt+PgUp, Win+Alt+PgDn and Win+Alt+Q) with Windows so you can adjust the volume and toggle Quiet mode. It does not log or record your keystrokes.

## 4. Information we collect

None. Bark does not collect:

- Names, email addresses, or other contact details
- Device identifiers, location, or hardware information
- Usage statistics, crash reports, or diagnostics
- Files, documents, audio, or screen content
- Any data from other applications

## 5. Network activity

**Microsoft Store edition.** Bark makes no network connections. Updates are delivered by the Microsoft Store.

**GitHub edition.** Bark makes a single kind of network request: it contacts GitHub to check whether a newer release of the https://github.com/bonanto/bark repository is available. This request:

- goes only to GitHub, for this repository's release information;
- does not include any personal information, account details, or identifiers created by Bark;
- is subject to the ordinary technical information inherent to any web request, such as your IP address, which GitHub receives and handles under the [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).

Bark does not send any other data anywhere, and Bonanto does not operate any server that receives information from Bark.

## 6. Information stored on your device

Bark keeps a small amount of data locally on your computer so it can work as intended, such as the volume level you chose, your Quiet mode setting, and whether Bark starts with Windows. This information stays on your device. It is never sent to us or to any third party.

## 7. Permissions

The Microsoft Store edition is a standard Windows desktop application packaged as MSIX. Its package declares the following capabilities:

| Capability | Purpose |
|---|---|
| `runFullTrust` | Bark is a native desktop application that runs in the system tray and must run outside the app sandbox. |
| `unvirtualizedResources` | Allows Bark to write the Windows setting for its own notification-area (tray) icon so the icon behaves correctly. |
| Startup task | Allows Bark to start when you sign in to Windows. You can turn this off at any time in **Task Manager → Startup apps** or **Settings → Apps → Startup**. |

These permissions are used only to operate Bark on your device. They are not used to collect or transmit data.

## 8. Third parties

Bark contains no third-party analytics, advertising, tracking, or data-sharing components. We do not share, sell, rent, or disclose personal information, because we do not collect any.

Two third parties are involved only in distribution:

- **Microsoft** operates the Microsoft Store and may collect information about your download and installation under the [Microsoft Privacy Statement](https://privacy.microsoft.com/privacystatement).
- **GitHub** hosts the Releases page and receives the update-check request described in section 5.

Neither receives any information from Bark beyond what is described above.

## 9. Children's privacy

Bark does not knowingly collect personal information from anyone, including children. It collects no data from any user.

## 10. Security

Bark holds no personal data about you, so there is no collected data for us to protect or lose. Settings stored on your device are protected by your Windows user account and your device's security.

## 11. Your rights

Since we hold no personal data about you, there is nothing for us to access, correct, export, or delete. You control Bark's local settings and can uninstall it at any time from **Settings → Apps → Installed apps**.

## 12. Changes to this policy

If this policy changes, the "Last updated" date will be revised and the new version published at this address. Significant changes will be mentioned in the release notes.

## 13. Contact

For questions about this policy, please [open an issue](https://github.com/bonanto/bark/issues) on the Bark repository.

**Bonanto** · https://github.com/bonanto/bark
