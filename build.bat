@echo off
setlocal
title Prompt Saver - Build
cd /d "%~dp0"

echo ============================================
echo  Prompt Saver - Portable EXE Build
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
rem Output fully silenced: tauri icon always generates Android/iOS sets too
rem and logs them on stderr - irrelevant noise for this Windows-only tool.
call npm run tauri -- icon ui/assets/icon.svg >nul 2>nul
if errorlevel 1 echo [WARN] Icon generation failed - exe keeps the previous icon.
if not exist "src-tauri\icons\icon.ico" echo [WARN] icons\icon.ico missing!
rem Windows-only project: drop generated mobile/macOS/Store icon sets right away
rem (NSIS + portable exe only use icon.ico and the plain PNG sizes).
if exist "src-tauri\icons\android" rmdir /s /q "src-tauri\icons\android"
if exist "src-tauri\icons\ios" rmdir /s /q "src-tauri\icons\ios"
if exist "src-tauri\icons\icon.icns" del /q "src-tauri\icons\icon.icns"
del /q "src-tauri\icons\Square*.png" 2>nul
del /q "src-tauri\icons\StoreLogo.png" 2>nul

echo [INFO] Building release exe ^(this can take a few minutes^)...
echo.
call npm run build -- --no-bundle
if errorlevel 1 goto :fail

set "EXE=%~dp0src-tauri\target\release\prompt-saver.exe"
if not exist "%EXE%" (
    echo [ERROR] Build finished but exe not found: %EXE%
    goto :fail
)

for %%A in ("%EXE%") do set "SIZE=%%~zA"
set /a SIZE_MB=%SIZE% / 1048576

echo.
echo ============================================
echo  DONE: %EXE%
echo  Size: ~%SIZE_MB% MB
echo ============================================
echo.
choice /c YN /m "Open output folder"
if errorlevel 2 goto :end
explorer /select,"%EXE%"
goto :end

:fail
echo.
echo Build FAILED. Check the messages above.
pause
exit /b 1

:end
pause
exit /b 0
