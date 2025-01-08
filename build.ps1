param (
    [switch]$Release
)

if ($Release) {
    cargo build --release
    New-Item -ItemType Directory -Force -Path ".\tmp"
    Copy-Item ".\target\release\battle_instinct.dll" -Destination ".\tmp\dinput8.dll"

    Copy-Item -Force ".\battle_instinct.cfg" -Destination ".\tmp\battle_instinct.cfg"
    Compress-Archive -Update -LiteralPath @(".\tmp\battle_instinct.cfg", ".\tmp\dinput8.dll") -CompressionLevel "NoCompression" -DestinationPath ".\battle-instinct_x64.zip"

    Copy-Item -Force ".\battle_instinct_zh.cfg" -Destination ".\tmp\battle_instinct.cfg"
    Compress-Archive -Update -LiteralPath @(".\tmp\battle_instinct.cfg", ".\tmp\dinput8.dll") -CompressionLevel "NoCompression" -DestinationPath ".\battle-instinct_zh_x64.zip"
    
    Remove-Item -Recurse ".\tmp"
} else {
    cargo build
    Copy-Item ".\target\debug\battle_instinct.dll" -Destination "C:\Program Files (x86)\Steam\steamapps\common\Sekiro\dinput8.dll"
    Copy-Item ".\battle_instinct.cfg" -Destination "C:\Program Files (x86)\Steam\steamapps\common\Sekiro\battle_instinct.cfg"
}

