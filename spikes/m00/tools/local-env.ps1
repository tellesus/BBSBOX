# SPDX-License-Identifier: AGPL-3.0-or-later
# Optional helpers for this workspace's portable tools. No machine PATH changes.
$bbsboxRoot = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
if (Test-Path (Join-Path $bbsboxRoot '.tooling/cargo/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $bbsboxRoot '.tooling/cargo'
    $env:RUSTUP_HOME = Join-Path $bbsboxRoot '.tooling/rustup'
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
$bbsboxMingw = Get-ChildItem (Join-Path $bbsboxRoot '.tooling') -Directory -Filter 'llvm-mingw-*-ucrt-x86_64' -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -First 1
if ($bbsboxMingw) {
    $env:PATH = "$($bbsboxMingw.FullName)\bin;$env:PATH"
    $env:CC = 'clang'
    $env:AR = 'llvm-ar'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = 'clang'
    $env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnullvm'
    $bbsboxCompiler = & rustc --version
    if ($bbsboxCompiler -notmatch '^rustc 1\.99\.1 ') {
        throw 'The portable toolchain must match pinned Rust 1.99.1. Update the local toolchain setup before building.'
    }
}
