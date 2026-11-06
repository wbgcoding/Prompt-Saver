# build-stamp.ps1
# © 2026 BGCoding
#
# Build freshness helper: lets a build script skip a rebuild when the last successful
# build is still current, meaning nothing in the project changed since.
#
#   build-stamp.ps1 -Check -Project <dir> -Key <name> [-Extra <text>]
#       Fresh: exit 0 and print the recorded artifact path(s), one per line.
#       Not fresh (or any internal problem): exit 1, nothing on stdout.
#       Add -Verbose to get a short reason on stderr.
#   build-stamp.ps1 -Save -Project <dir> -Key <name> -Artifact <path or wildcard>[,<more>] [-Extra <text>] [-Since <time>]
#       Records the stamp after a successful build. A wildcard resolves to the newest match.
#       Exit 1 when no artifact is found, or when an artifact is older than -Since.
#       -Since is an ISO time (UTC unless it carries an offset) or a FILETIME tick count.
#   build-stamp.ps1 -Clear -Project <dir> [-Key <name>]
#       Forgets the stamps of a project (or only those of one key).
#   build-stamp.ps1 -Now
#       Prints the current UTC time in a form that -Since accepts.
#
# Key is the kind of build (exe, phone, installer). Extra is the build mode or arguments,
# so a different mode never counts as fresh.
#
# The fingerprint covers the project sources, not build output:
#   git repo: HEAD commit plus path and content hash of every changed or new, non-ignored file.
#   otherwise: path, size and modification time of all files outside the usual output folders.
#   both: content of local, usually ignored build inputs (signing keys and their settings,
#   local.properties, .env files, export presets) plus any -Include <path or wildcard>.
#   Pass -Include with the same values to -Check and -Save.
# Stamps live in %LOCALAPPDATA%\BuildStamps, one json file per project.

[CmdletBinding()]
param(
    [switch]$Check,
    [switch]$Save,
    [switch]$Clear,
    [switch]$Now,
    [string]$Project,
    [string]$Key,
    [string]$Extra = '',
    [string[]]$Artifact,
    [string]$Since,
    [string[]]$Include
)

$ErrorActionPreference = 'Stop'
$script:Verb = ($VerbosePreference -eq 'Continue')
if ($null -eq $Extra) { $Extra = '' }

function Say([string]$msg) {
    if ($script:Verb) { [Console]::Error.WriteLine('build-stamp: ' + $msg) }
}

function ToHex([byte[]]$bytes) {
    [BitConverter]::ToString($bytes).Replace('-', '').ToLowerInvariant()
}

function HashText([string]$text, [string]$algorithm) {
    $h = [Security.Cryptography.HashAlgorithm]::Create($algorithm)
    try { ToHex ($h.ComputeHash([Text.Encoding]::UTF8.GetBytes($text))) } finally { $h.Dispose() }
}

function HashFile([string]$path, [string]$algorithm) {
    $h = [Security.Cryptography.HashAlgorithm]::Create($algorithm)
    $fs = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try { ToHex ($h.ComputeHash($fs)) } finally { $fs.Dispose(); $h.Dispose() }
}

function NormalizeDir([string]$dir) {
    # A trailing backslash before a closing quote from cmd leaves a stray quote behind.
    $d = $dir.Trim().Trim('"').Trim()
    $full = [IO.Path]::GetFullPath($d)
    if ($full.Length -gt 3) { $full = $full.TrimEnd('\') }
    $full
}

function RunGit([string]$dir, [string[]]$gitArgs) {
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $script:GitExe
    $psi.Arguments = ($gitArgs -join ' ')
    $psi.WorkingDirectory = $dir
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.StandardOutputEncoding = New-Object Text.UTF8Encoding($false)
    $p = [Diagnostics.Process]::Start($psi)
    $errTask = $p.StandardError.ReadToEndAsync()
    $out = $p.StandardOutput.ReadToEnd()
    $p.WaitForExit()
    [void]$errTask.Result
    [pscustomobject]@{ Code = $p.ExitCode; Out = $out }
}

function GitRoot([string]$dir) {
    # Returns the repository root only when it is exactly the project folder, else $null.
    $cmd = Get-Command git -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $cmd) { return $null }
    $script:GitExe = $cmd.Source
    try {
        $r = RunGit $dir @('rev-parse', '--show-toplevel')
    } catch { return $null }
    if ($r.Code -ne 0) { return $null }
    $top = NormalizeDir ($r.Out.Trim())
    if ($top -ieq $dir) { return $top }
    $null
}

function GitFingerprintText([string]$root) {
    $head = RunGit $root @('rev-parse', '--verify', '-q', 'HEAD')
    $headId = if ($head.Code -eq 0) { $head.Out.Trim() } else { 'nohead' }
    $st = RunGit $root @('--no-optional-locks', 'status', '--porcelain=v1', '-z', '--untracked-files=all')
    if ($st.Code -ne 0) { throw 'git status failed' }
    $sha1 = [Security.Cryptography.SHA1]::Create()
    $lines = New-Object 'Collections.Generic.List[string]'
    try {
        $parts = $st.Out.Split([char]0)
        for ($i = 0; $i -lt $parts.Length; $i++) {
            $e = $parts[$i]
            if ($e.Length -lt 4) { continue }
            $x = $e[0]; $y = $e[1]
            $rel = $e.Substring(3)
            # A rename or copy entry carries the old path as the next field.
            if ($x -eq 'R' -or $x -eq 'C' -or $y -eq 'R' -or $y -eq 'C') { $i++ }
            $full = Join-Path $root $rel
            if ([IO.File]::Exists($full)) {
                $mark = ToHex ($sha1.ComputeHash([IO.File]::ReadAllBytes($full)))
            } elseif ([IO.Directory]::Exists($full)) {
                $mark = 'dir:' + $x + $y
            } else {
                $mark = 'deleted'
            }
            $lines.Add($rel + '|' + $mark)
        }
    } finally { $sha1.Dispose() }
    $lines.Sort([StringComparer]::Ordinal)
    'git' + "`n" + $headId + "`n" + ($lines -join "`n")
}

function WalkFingerprintText([string]$root) {
    $skip = @{}
    foreach ($n in 'build', 'bin', 'obj', 'dist', 'out', 'node_modules', '.gradle', '.dart_tool', 'target', '.git', '.tmp', '.godot') {
        $skip[$n] = $true
    }
    $lines = New-Object 'Collections.Generic.List[string]'
    $stack = New-Object 'Collections.Generic.Stack[string]'
    $stack.Push($root)
    $cut = $root.Length
    while ($stack.Count -gt 0) {
        $di = New-Object IO.DirectoryInfo($stack.Pop())
        foreach ($e in $di.EnumerateFileSystemInfos()) {
            if ($e -is [IO.DirectoryInfo]) {
                if (($e.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { continue }
                if ($skip.ContainsKey($e.Name)) { continue }
                $stack.Push($e.FullName)
            } else {
                $lines.Add($e.FullName.Substring($cut) + '|' + $e.Length + '|' + $e.LastWriteTimeUtc.Ticks)
            }
        }
    }
    $lines.Sort([StringComparer]::Ordinal)
    'walk' + "`n" + ($lines -join "`n")
}

function ConfigFingerprintText([string]$root) {
    # Local files that change the output but are usually git-ignored: signing keys and their
    # settings, SDK paths, env files. Their presence and content count, so adding or swapping
    # a signing key forces a rebuild. -Include adds project specific paths or wildcards.
    $specs = @('key.properties', 'local.properties', '.env', '.env.*', '*.jks', '*.keystore',
               '.signing\*', 'android\key.properties', 'android\local.properties',
               'android\app\*.jks', 'android\app\*.keystore', 'app\*.jks', 'app\*.keystore',
               'export_presets.cfg', 'signing.properties') + @($Include | Where-Object { $_ })
    $lines = New-Object 'Collections.Generic.List[string]'
    $sha1 = [Security.Cryptography.SHA1]::Create()
    try {
        foreach ($s in $specs) {
            $dir = Join-Path $root (Split-Path $s -Parent)
            $leaf = Split-Path $s -Leaf
            if (-not [IO.Directory]::Exists($dir)) { continue }
            foreach ($f in [IO.Directory]::EnumerateFiles($dir, $leaf)) {
                $lines.Add($f.Substring($root.Length) + '|' + (ToHex ($sha1.ComputeHash([IO.File]::ReadAllBytes($f)))))
            }
        }
    } finally { $sha1.Dispose() }
    $lines.Sort([StringComparer]::Ordinal)
    'config' + "`n" + (($lines | Select-Object -Unique) -join "`n")
}

function GetFingerprint([string]$root) {
    $gitRoot = GitRoot $root
    $body = if ($gitRoot) { GitFingerprintText $gitRoot } else { WalkFingerprintText $root }
    $body = $body + "`n" + (ConfigFingerprintText $root)
    HashText ('v1' + "`n" + 'key=' + $Key + "`n" + 'extra=' + $Extra + "`n" + $body) 'SHA256'
}

function StampPath([string]$root) {
    $base = $env:LOCALAPPDATA
    if ([string]::IsNullOrEmpty($base)) { throw 'no local app data folder' }
    Join-Path (Join-Path $base 'BuildStamps') ((HashText $root.ToLowerInvariant() 'SHA1') + '.json')
}

function ReadStamp([string]$path) {
    # Anything unreadable counts as no stamp.
    try {
        if (-not [IO.File]::Exists($path)) { return $null }
        $obj = [IO.File]::ReadAllText($path, [Text.Encoding]::UTF8) | ConvertFrom-Json
        if ($null -eq $obj -or $obj.version -ne 1) { return $null }
        $obj
    } catch { $null }
}

function WriteStamp([string]$path, $stamp) {
    $dir = [IO.Path]::GetDirectoryName($path)
    [void][IO.Directory]::CreateDirectory($dir)
    $tmp = $path + '.' + [Guid]::NewGuid().ToString('N') + '.tmp'
    $json = $stamp | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText($tmp, $json, (New-Object Text.UTF8Encoding($false)))
    try {
        if ([IO.File]::Exists($path)) { [IO.File]::Replace($tmp, $path, [NullString]::Value) }
        else { [IO.File]::Move($tmp, $path) }
    } finally {
        if ([IO.File]::Exists($tmp)) { [IO.File]::Delete($tmp) }
    }
}

function ParseSince([string]$text) {
    if ($text -match '^\d+$') { return [DateTime]::FromFileTimeUtc([int64]$text) }
    $style = [Globalization.DateTimeStyles]::AssumeLocal -bor [Globalization.DateTimeStyles]::AdjustToUniversal
    [DateTime]::Parse($text, [Globalization.CultureInfo]::InvariantCulture, $style)
}

function ResolveArtifacts([string[]]$specs, [string]$root) {
    $found = New-Object 'Collections.Generic.List[IO.FileInfo]'
    foreach ($spec in $specs) {
        $items = @()
        $whole = $spec.Trim().Trim('"')
        if ($whole -ne '') { $items = @($whole) }
        # A comma may belong to the path; split only when the whole text matches nothing.
        if (-not (MatchArtifact $whole $root)) { $items = @($whole.Split(',') | ForEach-Object { $_.Trim().Trim('"') } | Where-Object { $_ -ne '' }) }
        foreach ($item in $items) {
            $m = MatchArtifact $item $root
            if ($m) { $found.Add($m) } else { Say ('no artifact matches ' + $item) ; return $null }
        }
    }
    $found
}

function MatchArtifact([string]$spec, [string]$root) {
    if ([string]::IsNullOrWhiteSpace($spec)) { return $null }
    $p = if ([IO.Path]::IsPathRooted($spec)) { $spec } else { Join-Path $root $spec }
    if ([IO.File]::Exists($p)) { return (New-Object IO.FileInfo([IO.Path]::GetFullPath($p))) }
    $dir = [IO.Path]::GetDirectoryName($p)
    $pat = [IO.Path]::GetFileName($p)
    if (-not $dir -or -not [IO.Directory]::Exists($dir) -or $pat -notmatch '[\*\?]') { return $null }
    $best = $null
    foreach ($f in (New-Object IO.DirectoryInfo($dir)).EnumerateFiles($pat)) {
        if ($null -eq $best -or $f.LastWriteTimeUtc -gt $best.LastWriteTimeUtc) { $best = $f }
    }
    $best
}

function DoCheck([string]$root) {
    if ([string]::IsNullOrEmpty($Key)) { Say 'no key'; return 1 }
    $stamp = ReadStamp (StampPath $root)
    if (-not $stamp) { Say 'no usable stamp'; return 1 }
    $entry = @($stamp.entries) | Where-Object { [string]$_.key -ceq $Key -and [string]$_.extra -ceq $Extra } | Select-Object -First 1
    if (-not $entry) { Say 'no stamp for this key and extra'; return 1 }
    $arts = @($entry.artifacts)
    if ($arts.Count -eq 0) { Say 'stamp has no artifact'; return 1 }
    # Cheap artifact checks first, the fingerprint walk last.
    foreach ($a in $arts) {
        $fi = New-Object IO.FileInfo([string]$a.path)
        if (-not $fi.Exists) { Say ('artifact missing: ' + $a.path); return 1 }
        if ($fi.Length -ne [int64]$a.size) { Say ('artifact size changed: ' + $a.path); return 1 }
        if ($fi.LastWriteTimeUtc.Ticks -ne [int64]$a.mtime) {
            if ((HashFile $fi.FullName 'SHA256') -ne [string]$a.sha256) { Say ('artifact content changed: ' + $a.path); return 1 }
        }
    }
    if ((GetFingerprint $root) -ne [string]$entry.fingerprint) { Say 'sources changed since the last build'; return 1 }
    foreach ($a in $arts) { [Console]::Out.WriteLine([string]$a.path) }
    0
}

function DoSave([string]$root) {
    if ([string]::IsNullOrEmpty($Key)) { [Console]::Error.WriteLine('build-stamp: -Key is required'); return 2 }
    if (-not $Artifact -or $Artifact.Count -eq 0) { [Console]::Error.WriteLine('build-stamp: -Artifact is required'); return 2 }
    $files = ResolveArtifacts $Artifact $root
    if (-not $files -or $files.Count -eq 0) { Say 'no artifact found'; return 1 }
    $sinceUtc = $null
    if ($Since) { $sinceUtc = ParseSince $Since }
    $recs = @()
    foreach ($f in $files) {
        if ($null -ne $sinceUtc -and $f.LastWriteTimeUtc -lt $sinceUtc) {
            Say ('artifact is older than the build start: ' + $f.FullName); return 1
        }
        $recs += [ordered]@{
            path = $f.FullName; size = $f.Length; mtime = $f.LastWriteTimeUtc.Ticks
            sha256 = (HashFile $f.FullName 'SHA256')
        }
    }
    $fp = GetFingerprint $root
    $path = StampPath $root
    $old = ReadStamp $path
    $entries = @()
    if ($old) {
        $entries = @(@($old.entries) | Where-Object { -not ([string]$_.key -ceq $Key -and [string]$_.extra -ceq $Extra) } |
            ForEach-Object { [ordered]@{ key = [string]$_.key; extra = [string]$_.extra; fingerprint = [string]$_.fingerprint; time = [string]$_.time; artifacts = @($_.artifacts) } })
    }
    $entries += [ordered]@{
        key = $Key; extra = $Extra; fingerprint = $fp
        time = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        artifacts = $recs
    }
    WriteStamp $path ([ordered]@{ version = 1; project = $root; entries = $entries })
    Say ('saved ' + $files.Count + ' artifact(s)')
    0
}

function DoClear([string]$root) {
    $path = StampPath $root
    if ([string]::IsNullOrEmpty($Key)) {
        if ([IO.File]::Exists($path)) { [IO.File]::Delete($path) }
        return 0
    }
    $old = ReadStamp $path
    if (-not $old) { return 0 }
    $keep = @(@($old.entries) | Where-Object { [string]$_.key -cne $Key } |
        ForEach-Object { [ordered]@{ key = [string]$_.key; extra = [string]$_.extra; fingerprint = [string]$_.fingerprint; time = [string]$_.time; artifacts = @($_.artifacts) } })
    if ($keep.Count -eq 0) { if ([IO.File]::Exists($path)) { [IO.File]::Delete($path) } }
    else { WriteStamp $path ([ordered]@{ version = 1; project = $old.project; entries = $keep }) }
    0
}

$mode = @($Check, $Save, $Clear, $Now | Where-Object { $_ }).Count
if ($mode -ne 1) {
    [Console]::Error.WriteLine('build-stamp: give exactly one of -Check, -Save, -Clear, -Now')
    exit $(if ($Check) { 1 } else { 2 })
}
if ($Now) {
    [Console]::Out.WriteLine((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffffffZ'))
    exit 0
}

$code = 1
try {
    if ([string]::IsNullOrWhiteSpace($Project)) { throw '-Project is required' }
    $root = NormalizeDir $Project
    if (-not [IO.Directory]::Exists($root)) { throw ('project folder not found: ' + $root) }
    if ($Check) { $code = DoCheck $root }
    elseif ($Save) { $code = DoSave $root }
    else { $code = DoClear $root }
} catch {
    $code = 1
    if ($Check) { Say ('error: ' + $_.Exception.Message) }
    else { [Console]::Error.WriteLine('build-stamp: ' + $_.Exception.Message) }
}
exit $code
