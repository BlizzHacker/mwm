# MWM - Move Weight Manager Privacy Policy

_Last updated: 2026-09-24_

MWM is an open-source system utility for Windows and Linux. The desktop app and Linux agent do not include advertising or product analytics. MWM does not operate a central service that collects the contents of your devices. The public project website is separate from a private manager installation.

## Data the app can access

At your request, MWM can read device information, files and file metadata, installed software, system logs, network and service status, and sensitive local items such as license keys, saved Wi-Fi passwords, and BitLocker recovery keys. The exact data depends on which page or command you use and the permissions available to MWM. Exports are written to the location you choose.

## Remote management

If you configure MWM's web manager, agents, Proxmox connection, SSH, or service plugins, requests and results travel between the devices and services you select. These results can include sensitive system information. MWM's web interface requires an access token. Its built-in HTTP listener is not encrypted; use a trusted local network or a correctly configured HTTPS reverse proxy or VPN. If you expose a private manager through an identity provider such as Authentik, that provider and your reverse proxy process sign-in and access logs under their own configuration.

Optional integrations such as MCP-ARR, Arkana, and media managers use the endpoints and credentials you configure. Their operators may process the data you send them according to their own policies. MWM does not send credentials to the public project landing page.

## Local storage and retention

MWM saves its settings, job state, and backups on the device where it runs. Windows settings are stored under `%LOCALAPPDATA%\MoveWeight\MWM`; Linux settings may be stored under `~/.local/share/MWM` or `/var/lib/MWM`. Configured agents, reverse proxies, and integrations can keep their own logs and state. Remove MWM's local data and integration configuration to end that storage; normal system backups can retain copies.

## Network requests

MWM makes network requests when you use network diagnostics, software updates, remote management, or integrations. Diagnostics can contact your router, public DNS or internet test endpoints. Package managers contact their configured repositories. Your network and service operators may log those requests.

## Contact and changes

For privacy questions, open an issue at https://github.com/BlizzHacker/mwm/issues. Changes to this policy are recorded in the project repository at https://github.com/BlizzHacker/mwm.
