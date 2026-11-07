$ErrorActionPreference = 'Stop'

$app = Join-Path $PSScriptRoot '../apps/rusty-brick-chess'
$frontend = Join-Path $app 'frontend'
$public = Join-Path $frontend 'public'

Push-Location $frontend
try {
    cargo leptos build --frontend-only --cargo-locked
    if ($LASTEXITCODE -ne 0) { throw 'RBC frontend build failed.' }
} finally {
    Pop-Location
}

Copy-Item (Join-Path $frontend 'index.html') (Join-Path $public 'index.html') -Force
$sounds = Join-Path $public 'sounds'
New-Item -ItemType Directory -Path $sounds -Force | Out-Null
Copy-Item (Join-Path $frontend 'assets/sounds/*') $sounds -Recurse -Force
Copy-Item (Join-Path $public 'pkg/rbc-frontend.wasm') (Join-Path $public 'pkg/rbc-frontend_bg.wasm') -Force

$previousStatic = $env:RBC_STATIC
$previousDb = $env:RBC_DB
try {
    $env:RBC_STATIC = (Resolve-Path $public).Path
    if (!$env:RBC_DB -and $env:LOCALAPPDATA) {
        $env:RBC_DB = Join-Path $env:LOCALAPPDATA 'RustyBrickChess/rusty-brick-chess.db'
    }
    cargo run --release --locked --manifest-path (Join-Path $app 'Cargo.toml') --bin rusty-brick-chess -- --open
    if ($LASTEXITCODE -ne 0) { throw 'RBC server exited with an error.' }
} finally {
    $env:RBC_STATIC = $previousStatic
    $env:RBC_DB = $previousDb
}
