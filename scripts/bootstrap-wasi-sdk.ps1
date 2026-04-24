$ErrorActionPreference = 'Stop'

$version = '32.0'
$release = 'wasi-sdk-32'
$archiveName = "wasi-sdk-$version-x86_64-windows.tar.gz"
$url = "https://github.com/WebAssembly/wasi-sdk/releases/download/$release/$archiveName"

$repoRoot = Split-Path -Parent $PSScriptRoot
$archive = Join-Path $repoRoot 'wasi-sdk.tar.gz'
$sdkDir = Join-Path $repoRoot 'wasi-sdk'
$extracted = Join-Path $repoRoot "wasi-sdk-$version-x86_64-windows"

if (Test-Path $sdkDir) {
    Remove-Item -Recurse -Force $sdkDir
}

Invoke-WebRequest -Uri $url -OutFile $archive
& "$env:SystemRoot\System32\tar.exe" -xzf $archive -C $repoRoot
Remove-Item $archive

if (Test-Path $sdkDir) {
    Remove-Item -Recurse -Force $sdkDir
}

Move-Item $extracted $sdkDir
