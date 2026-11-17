@echo off
cd /d "%~dp0"
if exist "target\release\soar-github-uploader.exe" (
    start "" "target\release\soar-github-uploader.exe"
) else (
    cargo run --release
)
