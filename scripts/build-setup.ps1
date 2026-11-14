# Builds openiCUE, then a Setup that embeds the fresh binaries and PawnIO.
# Not 'Stop': Windows PowerShell treats cargo's progress on stderr as an error.
# Each step checks $LASTEXITCODE instead.
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo build --release -p openicue -p openicue-engine
if ($LASTEXITCODE -ne 0) { exit 1 }
cargo build --release -p openicue-setup --features payload
if ($LASTEXITCODE -ne 0) { exit 1 }
Get-Item target\release\openiCUE-Setup.exe | Select-Object Name, Length, LastWriteTime
