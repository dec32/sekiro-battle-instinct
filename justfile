install *args:
    cargo build {{args}}
    cp "./target/debug/sekiro_battle_instinct.dll" "C:/Program Files (x86)/Steam/steamapps/common/Sekiro/dinput8.dll"
logs:
    tail -f "C:/Program Files (x86)/Steam/steamapps/common/Sekiro/battle_instinct.log"
pack:
    cargo build
    mkdir -p "./tmp"
    cp "./target/release/sekiro_battle_instinct.dll" "./tmp/dinput8.dll"

    cp -f "./res/battle_instinct.cfg" "./tmp/battle_instinct.cfg"
    7z a -tzip -mx9 "./battle-instinct.zip" "./tmp/dinput8.dll" "./tmp/battle_instinct.cfg"

    cp -f "./res/battle_instinct_zh.cfg" "./tmp/battle_instinct.cfg"
    7z a -tzip -mx9 "./battle-instinct_zh.zip" "./tmp/dinput8.dll" "./tmp/battle_instinct.cfg"

    rm -rf "./tmp"
release:
    just pack
    git tag -d nightly || true
    git push --delete origin nightly || true
    gh release create nightly "./battle-instinct.zip" "./battle-instinct_zh.zip" -t "Nightly Build" -n "Nightly Build"
