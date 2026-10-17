# Security policy

SilkScroll installs a system-wide mouse hook and injects input, so we take security reports seriously.

## Supported versions

Only the latest release receives fixes.

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Instead, use GitHub's private reporting: go to the
repository's **Security** tab and choose **Report a vulnerability**. Include the version, steps to reproduce, and
the impact you expect.

You can expect an acknowledgement within a week. We will keep you updated while we work on a fix and credit you
in the release notes unless you prefer otherwise.

## What SilkScroll does and doesn't do

- It acts on mouse wheel events only; every other mouse event passes straight through. While you scroll it
  checks whether Ctrl, Alt, Shift or Win is held. It does not record input and never reads keystrokes.
- It never opens a network connection.
- It runs with your normal user rights (`asInvoker`) and writes only to `%APPDATA%\SilkScroll` and, if you enable
  *Start with Windows*, one value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
