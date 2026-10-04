@echo off
setlocal enabledelayedexpansion
cd /d "%~dp0"

rem Product identity. GENERATED - run `node tools/brand.js <brand>` to change it;
rem the values live in brands\<brand>.json.
set "APP_NAME=Prompt Saver"
set "APP_SLUG=prompt-saver"

title %APP_NAME% - Build

echo ============================================
echo  %APP_NAME% - Build (installer + portable)
echo ============================================
echo.

rem Make sure cargo (Rust) is on PATH for this session.
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

rem A previous build may have left a deferred swap behind (see the dist\ copy
rem step below) because %APP_NAME%.exe was running at the time. Try the swap
rem again now, before spending time on a new build; if it is still running,
rem this is a silent no-op and the leftover file waits for the next attempt.
if exist "%~dp0dist\%APP_NAME%.new.exe" (
    move /y "%~dp0dist\%APP_NAME%.new.exe" "%~dp0dist\%APP_NAME%.exe" >nul 2>nul
)

rem A dev instance launched straight from this repo's own target\release locks
rem that exact exe file. Cargo can usually relink a running exe, but the NSIS
rem bundler copying that same file a moment later sometimes cannot (os error 32),
rem and by then the build has already burned its ~90 s compile time. Measured
rem 05.09.: an instance running from dist\ or from an installed copy does NOT
rem reproduce this (a full build completed with one of those running); only a
rem process executing from inside this repo's own target\release can, so that
rem is the one case worth catching early.
for /f "delims=" %%P in ('powershell -NoProfile -Command "(Get-Process -Name '%APP_SLUG%' -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '%~dp0src-tauri\target\release\*' } | Select-Object -First 1 -ExpandProperty Path)"') do set "DEV_INSTANCE=%%P"
if defined DEV_INSTANCE (
    echo [ERROR] %APP_NAME% is running straight from this repo's own build output:
    echo [ERROR]   %DEV_INSTANCE%
    echo [ERROR] Close it first, then build again - a build cannot replace an exe
    echo [ERROR] while it is executing from that exact path.
    goto :fail
)

where npm >nul 2>nul
if errorlevel 1 (
    echo [ERROR] npm not found. Install Node.js first: https://nodejs.org
    goto :fail
)

where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found. Install Rust first: https://rustup.rs
    goto :fail
)

if not exist "node_modules\" (
    echo [INFO] node_modules missing - running npm install...
    call npm install
    if errorlevel 1 goto :fail
)

echo [INFO] Regenerating app icons from ui\assets\icon.svg...
rem NOTE: "--" is required, otherwise npm swallows the path argument and
rem tauri icon silently falls back to ./app-icon.png (old exe icon kept).
call npm run tauri -- icon ui/assets/icon.svg >nul 2>nul
if errorlevel 1 echo [WARN] Icon generation failed - exe keeps the previous icon.
if not exist "src-tauri\icons\icon.ico" echo [WARN] icons\icon.ico missing!
rem Windows-only project: drop generated mobile/macOS/Store icon sets right away.
if exist "src-tauri\icons\android" rmdir /s /q "src-tauri\icons\android"
if exist "src-tauri\icons\ios" rmdir /s /q "src-tauri\icons\ios"
if exist "src-tauri\icons\icon.icns" del /q "src-tauri\icons\icon.icns"
del /q "src-tauri\icons\Square*.png" 2>nul
del /q "src-tauri\icons\StoreLogo.png" 2>nul

rem Installer icon = app logo + a small "install" badge (icons\installer-icon.ico).
echo [INFO] Regenerating installer icon...
call npm run tauri -- icon ui/assets/installer-icon.svg -o "%TEMP%\ps-instico" >nul 2>nul
if exist "%TEMP%\ps-instico\icon.ico" copy /y "%TEMP%\ps-instico\icon.ico" "src-tauri\icons\installer-icon.ico" >nul
if exist "%TEMP%\ps-instico" rmdir /s /q "%TEMP%\ps-instico"

echo [INFO] Building release exe + installer ^(this can take a few minutes^)...
echo.
rem Panic locations from the dependencies survive "strip" as string literals, which
rem would put the build machine's account name (%%USERPROFILE%%\.cargo\registry\...)
rem into a public download. Remap them to a neutral prefix. Changing RUSTFLAGS
rem invalidates the dependency cache, so the first build after this is a full one.
rem rustc splits RUSTFLAGS on spaces, so a raw profile path containing one breaks the
rem flag outright. Resolve the short (8.3) form first - it never contains a space -
rem and point CARGO_HOME at that same short alias (the identical on-disk folder, just
rem a different name for it) so the paths cargo/rustc actually embed match what
rem RUSTFLAGS remaps, instead of the remap silently not matching anything while the
rem long, space-containing path still ends up in the public exe.
for %%I in ("%USERPROFILE%") do set "USERPROFILE_SHORT=%%~sI"
set "CARGO_REG=%USERPROFILE_SHORT%\.cargo\registry"
echo %CARGO_REG%| find " " >nul
if not errorlevel 1 (
    echo [ERROR] Build machine user path still contains a space even in short ^(8.3^) form.
    echo [ERROR] Refusing to build a public release that would embed it - move the build
    echo [ERROR] environment to a path without spaces, or enable 8.3 name generation
    echo [ERROR] ^(fsutil 8dot3name^) for this volume.
    goto :fail
)
set "CARGO_HOME=%USERPROFILE_SHORT%\.cargo"
set "RUSTFLAGS=--remap-path-prefix=%CARGO_REG%=cargo"
rem Full bundle: produces BOTH the portable exe (target\release\<slug>.exe)
rem AND the NSIS installer (target\release\bundle\nsis). Never pass --no-bundle
rem here or the installer is skipped (that was the "no installer created" bug).
call npm run build
if errorlevel 1 (
    rem Measured 05.09.: neither a running instance nor the single-instance
    rem guard reproduces the historical "os error 32" bundling failure - a
    rem full build completed with the app running throughout. The remaining
    rem candidate is a transient lock on the NSIS staging folder (antivirus or
    rem an indexer scanning the freshly linked exe), which clears itself
    rem within seconds. One retry after wiping that staging folder covers it
    rem without silently swallowing a real, repeatable failure.
    echo [WARN] Build failed - clearing the NSIS staging folder and retrying once, in case a background scanner briefly locked a file in it...
    if exist "%~dp0src-tauri\target\release\bundle\nsis" rmdir /s /q "%~dp0src-tauri\target\release\bundle\nsis"
    call npm run build
    if errorlevel 1 goto :fail
)

set "EXE=%~dp0src-tauri\target\release\%APP_SLUG%.exe"
if not exist "%EXE%" (
    echo [ERROR] Build finished but exe not found: %EXE%
    goto :fail
)

rem Locate the freshly built NSIS installer (newest *-setup.exe).
set "INSTALLER="
for /f "delims=" %%F in ('dir /b /a-d /o-d "%~dp0src-tauri\target\release\bundle\nsis\*-setup.exe" 2^>nul') do (
    if not defined INSTALLER set "INSTALLER=%~dp0src-tauri\target\release\bundle\nsis\%%F"
)

rem Collect both artifacts in dist\ for convenience (clean, predictable names).
set "DIST=%~dp0dist"
if not exist "%DIST%" mkdir "%DIST%"
rem The portable exe in dist\ may itself be the copy that is running. Copying
rem straight onto a running exe can fail, so land the new build under a
rem temporary name first and only then try to swap it onto the final name -
rem a locked destination then leaves a clearly-named ".new.exe" behind instead
rem of failing the whole build.
copy /y "%EXE%" "%DIST%\%APP_NAME%.new.exe" >nul
if errorlevel 1 (
    echo [WARN] Could not stage the portable exe in dist - disk or permissions problem, leaving the old one in place.
) else (
    move /y "%DIST%\%APP_NAME%.new.exe" "%DIST%\%APP_NAME%.exe" >nul 2>nul
    if errorlevel 1 (
        echo [INFO] %APP_NAME%.exe is running. The new build is next to it as %APP_NAME%.new.exe and will be swapped in on the next build.
    )
)
if defined INSTALLER copy /y "%INSTALLER%" "%DIST%\" >nul
rem SHA-256 sidecar for the installer (the auto-updater verifies "<installer>.sha256"
rem before running the download). Upload it alongside the installer on the GitHub release.
if defined INSTALLER for %%I in ("%INSTALLER%") do powershell -NoProfile -Command "$h=(Get-FileHash -Algorithm SHA256 '%%~fI').Hash.ToLower(); [IO.File]::WriteAllText('%DIST%\%%~nxI.sha256', $h)"
rem pdfium.dll is compiled INTO the exe (see pdfium_lib_path in images.rs) and unpacked
rem on first use, so nothing has to travel next to the portable exe any more.
rem ffmpeg.exe is the opposite: it is NOT embedded (100+ MB). If src-tauri\ffmpeg.exe
rem is present, tauri.conf.json's bundle.resources ships it inside the installer
rem automatically; the portable exe here in dist\ never gets a copy and instead
rem offers to download ffmpeg itself on first need (see ffmpeg_search_path in ffmpeg.rs).

echo.
echo ============================================
echo  DONE
echo  Portable : %DIST%\%APP_NAME%.exe
if defined INSTALLER (
    echo  Installer: %INSTALLER%
) else (
    echo  [WARN] Installer not found - the bundle step may have failed.
)
echo ============================================
echo.
rem Automation / piped runs set PS_NONINTERACTIVE=1 to skip every interactive wait
rem (choice/timeout hang when there is no console attached).
if defined PS_NONINTERACTIVE goto :end
rem Offer to open the output folder, but never block: default to "no" after 10s
rem so the window can close itself unattended.
choice /c YN /t 10 /d N /m "Open output folder (closes automatically)"
if errorlevel 2 goto :end
explorer "%DIST%"
goto :end

:fail
echo.
echo Build FAILED. Check the messages above.
if defined PS_NONINTERACTIVE exit /b 1
echo This window closes automatically in 30 seconds...
timeout /t 30 >nul
exit /b 1

:end
echo.
if defined PS_NONINTERACTIVE exit /b 0
echo Closing automatically in 8 seconds...
timeout /t 8 >nul
exit /b 0
