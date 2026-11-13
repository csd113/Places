# Official, hash-pinned SDL and DXC; Visual C++ runtime from the hosted VS install.
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:RUNNER_TEMP 'places-runtime'
New-Item -ItemType Directory -Path "$root/licenses" -Force | Out-Null
function Get-VerifiedArchive($url, $sha, $name) {
    $zip = Join-Path $env:RUNNER_TEMP "$name.zip"
    Invoke-WebRequest $url -OutFile $zip
    if ((Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant() -ne $sha) {
        throw "Checksum mismatch: $name"
    }
    Expand-Archive $zip -DestinationPath (Join-Path $env:RUNNER_TEMP $name)
}
Get-VerifiedArchive 'https://github.com/libsdl-org/SDL/releases/download/release-3.4.18/SDL3-devel-3.4.18-VC.zip' '78d84602ae616cfe26b33a73b7d3b9b6b56e8a6650e53d92c9c6ca938f747acd' 'places-sdl'
Get-VerifiedArchive 'https://github.com/microsoft/DirectXShaderCompiler/releases/download/v1.9.2609/dxc_2026_09_29.zip' 'ad31b1fc8443175d204f77a611fdb3ef2ec42759bdc2f1167368de24a4a7e7f1' 'places-dxc'
$sdl = Join-Path $env:RUNNER_TEMP 'places-sdl/SDL3-3.4.18'
$dxc = Join-Path $env:RUNNER_TEMP 'places-dxc'
Copy-Item "$sdl/lib/x64/SDL3.dll" $root
Copy-Item "$sdl/LICENSE.txt" "$root/SDL3-LICENSE.txt"
Copy-Item "$dxc/bin/x64/dxcompiler.dll", "$dxc/bin/x64/dxil.dll" $root
Copy-Item "$dxc/LICENSE-LLVM.txt", "$dxc/LICENSE-MS.txt" "$root/licenses"
$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vs) { throw 'Visual Studio C++ tools are missing' }
$crt = Get-ChildItem "$vs/VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT" -Directory |
    Sort-Object FullName -Descending | Select-Object -First 1
if (!$crt) { throw 'Visual C++ x64 redistributable is missing' }
foreach ($dll in @('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')) {
    Copy-Item (Join-Path $crt.FullName $dll) $root
}
if (!(Test-Path "$vs/Licenses")) { throw 'Visual Studio license notices are missing' }
Copy-Item "$vs/Licenses" "$root/licenses/VisualStudio" -Recurse
@'
Visual C++ runtime DLLs are distributed from the licensed Visual Studio installation
on this GitHub-hosted runner. See the accompanying VisualStudio license notices.
Redistribution list: https://learn.microsoft.com/visualstudio/releases/2022/redistribution
DXC notices are LICENSE-MS.txt and LICENSE-LLVM.txt; SDL has SDL3-LICENSE.txt.
'@ | Set-Content "$root/licenses/RUNTIME-NOTICES.txt"
# sdl3-sys falls back to SDL3.lib when pkg-config is disabled; Rust's native
# search path uses the exact matching VC import library without a vcpkg rebuild.
"SDL3_NO_PKG_CONFIG=1" >> $env:GITHUB_ENV
"RUSTFLAGS=-Lnative=$sdl/lib/x64" >> $env:GITHUB_ENV
"PLACES_WINDOWS_RUNTIME=$root" >> $env:GITHUB_ENV
$root >> $env:GITHUB_PATH
