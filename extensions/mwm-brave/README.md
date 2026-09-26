# MWM Browser Extension

This Manifest V3 extension opens the existing Authentik-protected MWM web app at
`https://manage.moveweight.com/app/`. It has no permissions and stores no
credentials. The server and its SSO configuration provide remote access; the
extension is a shortcut, not a tunnel, VPN, password sync, or autofill provider.

In Brave, open `brave://extensions`, enable Developer mode, choose **Load
unpacked**, and select this folder. Pin **MWM Manager** from Brave's Extensions
menu. Chrome uses the same steps at `chrome://extensions`.
