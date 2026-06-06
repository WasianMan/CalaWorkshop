<#
.SYNOPSIS
  Package the extension/ folder into dev_wasian_calaworkshop.c7s.zip.

.DESCRIPTION
  Produces a Calagopus extension archive whose layout matches what the panel's
  installer validates: Metadata.toml + backend/ + frontend/ (+ migrations/) at
  the zip root, WITH explicit directory entries (Compress-Archive does not
  reliably create those, which makes the panel reject the archive).

  Drop the resulting .c7s.zip into your heavy panel's ./build/extensions and
  restart the web container, or upload it via the admin extensions page.

.NOTES
  This is a convenience packager. The canonical tool is
  `panel-rs extensions export dev.wasian.calaworkshop` in a dev environment,
  which produces the same structure.
#>

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root 'extension'
$distDir = Join-Path $root 'dist'
$outFile = Join-Path $distDir 'dev_wasian_calaworkshop.c7s.zip'

if (-not (Test-Path $source)) { throw "extension/ not found at $source" }
if (-not (Test-Path (Join-Path $source 'Metadata.toml'))) { throw 'extension/Metadata.toml missing' }

New-Item -ItemType Directory -Force -Path $distDir | Out-Null
if (Test-Path $outFile) { Remove-Item $outFile -Force }

# Paths excluded from the archive (build output, deps, vcs noise).
$excludeSegments = @('node_modules', 'target', '.git', 'dist')

function Test-Excluded([string]$relPath) {
    foreach ($seg in ($relPath -split '[\\/]+')) {
        if ($excludeSegments -contains $seg) { return $true }
    }
    return $false
}

$zip = [System.IO.Compression.ZipFile]::Open($outFile, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    # Explicit directory entries (trailing slash) — the validator checks for these.
    Get-ChildItem -Path $source -Recurse -Directory | ForEach-Object {
        $rel = $_.FullName.Substring($source.Length + 1)
        if (Test-Excluded $rel) { return }
        $entryName = ($rel -replace '\\', '/') + '/'
        $zip.CreateEntry($entryName) | Out-Null
    }

    Get-ChildItem -Path $source -Recurse -File | ForEach-Object {
        $rel = $_.FullName.Substring($source.Length + 1)
        if (Test-Excluded $rel) { return }
        $entryName = $rel -replace '\\', '/'
        [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $_.FullName, $entryName) | Out-Null
    }
}
finally {
    $zip.Dispose()
}

$size = [math]::Round((Get-Item $outFile).Length / 1KB, 1)
Write-Host "Built $outFile ($size KB)" -ForegroundColor Green
