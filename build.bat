@echo off
setlocal enabledelayedexpansion
title Prompt Saver - Build
cd /d "%~dp0"

echo ============================================
echo  Prompt Saver - Build (installer + portable)
echo ============================================
echo.

rem Make sure cargo (Rust) is on PATH for this session.
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

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
rem Full bundle: produces BOTH the portable exe (target\release\prompt-saver.exe)
rem AND the NSIS installer (target\release\bundle\nsis). Never pass --no-bundle
rem here or the installer is skipped (that was the "no installer created" bug).
call npm run build
if errorlevel 1 goto :fail

set "EXE=%~dp0src-tauri\target\release\prompt-saver.exe"
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
copy /y "%EXE%" "%DIST%\Prompt Saver.exe" >nul || echo [WARN] Could not update the portable exe in dist - is it currently running? Close it and rebuild.
if defined INSTALLER copy /y "%INSTALLER%" "%DIST%\" >nul
rem SHA-256 sidecar for the installer (the auto-updater verifies "<installer>.sha256"
rem before running the download). Upload it alongside the installer on the GitHub release.
if defined INSTALLER for %%I in ("%INSTALLER%") do powershell -NoProfile -Command "$h=(Get-FileHash -Algorithm SHA256 '%%~fI').Hash.ToLower(); [IO.File]::WriteAllText('%DIST%\%%~nxI.sha256', $h)"
rem pdfium.dll is compiled INTO the exe (see pdfium_lib_path in lib.rs) and unpacked
rem on first use, so nothing has to travel next to the portable exe any more.

echo.
echo ============================================
echo  DONE
echo  Portable : %DIST%\Prompt Saver.exe
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
