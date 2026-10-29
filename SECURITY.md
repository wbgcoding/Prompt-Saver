# Security policy

## Supported versions

Only the latest release receives security fixes.

## Reporting a vulnerability

Please do not open a public issue for a security problem. Report it privately through GitHub
instead: open the **Security** tab of this repository and choose **Report a vulnerability**,
or go straight to https://github.com/wbgcoding/Prompt-Saver/security/advisories/new.

Helpful details:

- the Prompt Saver version, installed or portable, and your Windows version
- what an attacker could do and the steps to reproduce it
- whether the problem needs a local user, a crafted file or a crafted backup

You get an answer within a few days. Once a fix is released, the report is credited in the
changelog unless you prefer to stay anonymous.

## Scope

Prompt Saver works offline. It only goes online to check GitHub for a new version, and it keeps
its data in `%APPDATA%\Prompt-Saver\` (installed) or beside the exe (portable). Reports about how
it stores or encrypts that data, imports files and backups, or verifies and installs an update
are all in scope.
