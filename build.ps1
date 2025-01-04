param (
    [switch]$Release
)

if ($Release) {
    cargo build --release
    Copy-Item ".\target\release\battle_instinct.dll" -Destination ".\dinput8.dll"
    Compress-Archive -Update -LiteralPath @(".\battle_instinct.cfg", ".\dinput8.dll") -CompressionLevel "NoCompression" -DestinationPath ".\battle-instinct_x64.zip"
    Remove-Item .\dinput8.dll
} else {
    cargo build
    Copy-Item ".\target\debug\battle_instinct.dll" -Destination "C:\Program Files (x86)\Steam\steamapps\common\Sekiro\dinput8.dll"
    Copy-Item ".\battle_instinct.cfg" -Destination "C:\Program Files (x86)\Steam\steamapps\common\Sekiro\battle_instinct.cfg"
}

