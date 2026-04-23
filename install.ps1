#!/usr/bin/env pwsh
# Install zacor on Windows.
#   irm https://raw.githubusercontent.com/Zacalot/zacor/main/install.ps1 | iex

$ErrorActionPreference = 'Stop'

$owner  = 'Zacalot'
$repo   = 'zacor'
$target = 'x86_64-pc-windows-msvc'

$installDir = Join-Path $env:USERPROFILE '.zacor\bin'
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

Write-Host 'Querying latest release...'
$release = Invoke-RestMethod "https://api.github.com/repos/$owner/$repo/releases/latest" `
    -Headers @{ 'User-Agent' = 'zacor-install' }
$tag     = $release.tag_name
$asset   = "zacor-$tag-$target.zip"
$baseUrl = "https://github.com/$owner/$repo/releases/download/$tag"

$zipUrl = "$baseUrl/$asset"
$shaUrl = "$zipUrl.sha256"

$tmp = New-Item -ItemType Directory -Force -Path (Join-Path $env:TEMP "zacor-install-$PID")
$zip = Join-Path $tmp $asset
$sha = "$zip.sha256"

try {
    Write-Host "Downloading $asset..."
    Invoke-WebRequest $zipUrl -OutFile $zip -UseBasicParsing
    Invoke-WebRequest $shaUrl -OutFile $sha -UseBasicParsing

    $expected = ((Get-Content $sha -Raw) -split '\s+')[0].ToLower()
    $actual   = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) {
        throw "Checksum mismatch: expected $expected, got $actual"
    }

    Write-Host "Extracting to $installDir..."
    Expand-Archive -Path $zip -DestinationPath $installDir -Force

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $paths    = if ($userPath) { $userPath -split ';' } else { @() }
    if ($paths -notcontains $installDir) {
        [Environment]::SetEnvironmentVariable('Path', (@($paths) + $installDir -join ';'), 'User')
        Write-Host "Added $installDir to user PATH."
    }

    Write-Host ''
    Write-Host "Installed zacor $tag."
    Write-Host 'Restart your shell, then run: zr --version'
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
