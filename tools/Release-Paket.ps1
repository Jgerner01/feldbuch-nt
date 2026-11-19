# Zweck:   Baut Feldbuch-NT in der Release-Konfiguration und erzeugt die beiden Dateien, die der Update-Dienst erwartet:
#            Feldbuch-NT-v<Version>-win-x64.zip  und  Feldbuch-NT-v<Version>-win-x64.zip.sha256
#          Die Version stammt aus Cargo.toml. Das Skript veröffentlicht nichts; die beiden Dateien aus dem Ordner "dist"
#          werden von Hand an ein GitHub-Release mit dem Tag v<Version> angehängt (Jgerner01/feldbuch-nt).
# Aufruf:  powershell -ExecutionPolicy Bypass -File tools\Release-Paket.ps1
# Autor:   Johann Gerner

param(
    [string]$Ausgabe = "dist",
    # Zusatz für Testversionen, z. B. "Test1": erscheint in der Versionsanzeige und im Dateinamen (Tag v<Version>-<Zusatz>)
    [string]$Zusatz = ""
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

$treffer = Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $treffer) { throw "Die Version wurde in Cargo.toml nicht gefunden." }
$version = $treffer.Matches[0].Groups[1].Value
$tag = "v$version"
if ($Zusatz) {
    $tag = "v$version-$Zusatz"
    $env:FELDBUCH_ZUSATZ = $Zusatz
} else {
    Remove-Item Env:FELDBUCH_ZUSATZ -ErrorAction SilentlyContinue
}

Write-Host "Baue Feldbuch-NT $version (Release) ..."
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build ist fehlgeschlagen." }

# Zielordner von cargo erfragen (berücksichtigt CARGO_TARGET_DIR und target-dir aus .cargo\config.toml;
# sonst läge eine veraltete EXE im Projektordner)
$zielordner = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
if (-not $zielordner) { throw "Der Zielordner von cargo konnte nicht ermittelt werden." }
$exe = Join-Path $zielordner "release\feldbuch-nt.exe"
if (-not (Test-Path $exe)) { throw "$exe wurde nicht erzeugt." }

New-Item -ItemType Directory -Force -Path $Ausgabe | Out-Null
$zipName = "Feldbuch-NT-$tag-win-x64.zip"
$zipPfad = Join-Path $Ausgabe $zipName
if (Test-Path $zipPfad) { Remove-Item $zipPfad }
Compress-Archive -Path $exe -DestinationPath $zipPfad

$hash = (Get-FileHash -Algorithm SHA256 -Path $zipPfad).Hash
# Format: "<Prüfsumme>  <Dateiname>", ohne Byte-Order-Mark
[System.IO.File]::WriteAllText((Join-Path (Resolve-Path $Ausgabe) "$zipName.sha256"), "$hash  $zipName`n", (New-Object System.Text.UTF8Encoding($false)))

Write-Host ""
Write-Host "Fertig:"
Write-Host "  $zipPfad  ($([math]::Round((Get-Item $zipPfad).Length / 1MB, 1)) MB)"
Write-Host "  $zipPfad.sha256  ($hash)"
Write-Host ""
Write-Host "Naechste Schritte: Tag $tag anlegen, ein GitHub-Release erstellen und beide Dateien anhaengen."
