curl -L -o libtorch.zip https://download.pytorch.org/libtorch/cu121/libtorch-win-shared-with-deps-2.2.0%2Bcu121.zip
unzip libtorch.zip
rm libtorch.zip

$directory = ".cargo"
if (-Not (Test-Path -Path $directory)) {
    New-Item -ItemType Directory -Force -Path $directory
}

$currentPath = (Get-Location).Path -replace '\\', '/'

$content = @"
[env]
LIBTORCH = "$currentPath/libtorch/"
Path = "$Path;$currentPath/libtorch/"
"@

Set-Content -Path ".cargo/config.toml" -Value $content
