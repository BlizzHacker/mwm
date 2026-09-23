# MWM - Move Weight Manager: Privacy Policy

_Last updated: 2026-09-23_

**MWM collects nothing.** It has no telemetry, no analytics, no crash reporting, no accounts and no ads.

- Everything MWM reads - files, installed programs, product keys, Wi-Fi passwords, BitLocker recovery keys,
  hardware details, logs - is read on your device, shown only to you, and never transmitted anywhere.
- MWM only uses the network when you ask it to: checking for software updates through Windows Package Manager
  (winget) or your Linux package manager, the network diagnostics page (which pings your router, 1.1.1.1 and
  resolves www.microsoft.com), and the optional `mwm serve` web interface, which listens only on the address
  you choose and requires an access token.
- Exports (system reports, key lists) are written only to a location you pick.
- Registry backups and settings are stored locally in `%LOCALAPPDATA%\MoveWeight\MWM` (Windows) or
  `~/.local/share/MWM` / `/var/lib/MWM` (Linux).

MWM is open source (GPL-3.0-or-later); you can verify all of the above in the code at
https://github.com/BlizzHacker/mwm. Questions: open an issue on that repository.
