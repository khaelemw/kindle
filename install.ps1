# Builds Kindle and installs it into the game folder.
#   .\install.ps1 [-GameDir "<DS3 install>\Game"] [-NoBuild]
# GameDir defaults to $env:KINDLE_GAME_DIR, then the standard Steam location.
# Never overwrites an existing kindle.toml or existing mods.
param(
    [string]$GameDir = $(if ($env:KINDLE_GAME_DIR) { $env:KINDLE_GAME_DIR } else { "C:\Program Files (x86)\Steam\steamapps\common\DARK SOULS III\Game" }),
    [switch]$NoBuild
)
$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

if (-not $NoBuild) {
    # Keep the user's name and folder layout out of the binaries: rewrite embedded source
    # paths (panic locations) to neutral prefixes and store only the PDB file name.
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
    # CARGO_ENCODED_RUSTFLAGS is separated by 0x1F, so paths may contain spaces.
    $env:CARGO_ENCODED_RUSTFLAGS = @(
        "--remap-path-prefix=$cargoHome=~/.cargo",
        "--remap-path-prefix=$root=kindle",
        "--remap-path-prefix=$env:USERPROFILE=~",
        "-Clink-arg=/PDBALTPATH:%_PDB%"
    ) -join [char]0x1F
    cargo build --release --manifest-path "$root\Cargo.toml"
    if ($LASTEXITCODE -ne 0) { throw "build failed" }
}

if (Get-Process DarkSoulsIII -ErrorAction SilentlyContinue) {
    throw "Dark Souls III is running - close it first (kindle.dll is in use)"
}

$dest = Join-Path $GameDir "kindle"
New-Item -ItemType Directory -Force "$dest\mods", "$dest\stubs", "$dest\logs" | Out-Null

Copy-Item "$root\target\release\kindle.dll" "$dest\kindle.dll" -Force
Copy-Item "$root\target\release\kindle-cli.exe" "$dest\kindle.exe" -Force
Copy-Item "$root\target\release\kindle-launcher.exe" "$dest\Kindle Launcher.exe" -Force
Copy-Item "$root\dist\stubs\*" "$dest\stubs\" -Force
Copy-Item "$root\docs\kindle-docs.html" "$dest\docs.html" -Force
Copy-Item "$root\THIRD_PARTY_NOTICES.md" "$dest\THIRD_PARTY_NOTICES.md" -Force
if (-not (Test-Path "$dest\kindle.toml")) { Copy-Item "$root\dist\kindle.toml" "$dest\kindle.toml" }

Get-ChildItem "$root\dist\mods" -Directory | ForEach-Object {
    $target = Join-Path "$dest\mods" $_.Name
    if (-not (Test-Path $target)) { Copy-Item $_.FullName $target -Recurse }
}

Write-Host "Kindle installed to $dest"
if (-not (Test-Path "$dest\launcher.toml")) {
    Write-Host "First time? Open Kindle Launcher.exe and press Find Dark Souls III (or run: kindle launch --setup)"
}
Write-Host "Start the game with Kindle: Kindle Launcher.exe, or run: kindle launch [profile]"
