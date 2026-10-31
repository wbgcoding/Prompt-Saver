# Privacy

Prompt Saver works on your computer. It has no account, no telemetry, no
analytics and no advertising, and the publisher never receives your prompts,
files, settings or usage data.

## What stays on your computer

Everything you create: prompts, notes, tags, views, images, recordings,
screenshots, backups and settings. The database is sealed with Windows' own
per-account encryption so other accounts on the same PC cannot read it.
Confidential prompts are additionally encrypted with their own vault password.
Installed, the data lives in `%APPDATA%\Prompt-Saver\`; the portable version
keeps it next to its exe. Uninstalling leaves this folder in place, so delete
it yourself if you want everything gone.

## When the app connects to the internet

Only in these cases, and never with your prompts or files:

| What | Who receives the request | What is sent | Can you avoid it? |
|---|---|---|---|
| Update check | GitHub (`api.github.com`, `github.com`) | A normal web request for the latest release; GitHub sees your IP address | Yes: switch automatic updates off in the settings |
| Installing an update you accepted | GitHub | The download request | Yes: decline the update |
| WebView2 runtime, only if missing | Microsoft (`go.microsoft.com`) | The download request, after you agree | Install WebView2 yourself |
| ffmpeg for video export, only if missing | GitHub (the pinned ffmpeg build) | The download request, after you agree | Yes: decline, or place ffmpeg yourself |
| Voice dictation | Microsoft's online speech recognition | Your voice audio while you dictate, after you agree | Yes: dictation is never used without your consent |
| Recognition or speech language packs | Windows Update | Windows' own download request, when you install a pack | Yes: install none |
| Links you click (project page, releases, this page) | GitHub, through your browser | A normal page visit | Yes |

Text recognition (OCR) runs fully offline through Windows' own recognizer.

## Third parties

GitHub and Microsoft process the requests above under their own privacy
policies. Prompt Saver passes them nothing beyond what the table lists.

## Contact

Questions or concerns: open an issue at
<https://github.com/wbgcoding/Prompt-Saver/issues>, or report a security
problem privately through the repository's security advisories.

© 2026 BGCoding
