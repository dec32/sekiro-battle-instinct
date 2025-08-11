fmt:
    cargo +nightly fmt --all

build:
    cargo build
    cp ./target/debug/battle_instinct.dll "C:\Program Files (x86)\Steam\steamapps\common\Sekiro\dinput8.dll"

release:
    cargo build --release
    cp ./target/release/battle_instinct.dll "C:\Program Files (x86)\Steam\steamapps\common\Sekiro\dinput8.dll"

    mkdir -p tmp
    cp ./target/release/battle_instinct.dll ./tmp/dinput8.dll

    cp -f ./res/battle_instinct.cfg ./tmp/battle_instinct.cfg
    zip -j -u -r battle-instinct.zip ./tmp/battle_instinct.cfg ./tmp/dinput8.dll

    cp -f ./res/battle_instinct_zh.cfg ./tmp/battle_instinct.cfg
    zip -j -u -r battle-instinct_zh.zip ./tmp/battle_instinct.cfg ./tmp/dinput8.dll

    rm -rf ./tmp

    git tag -d nightly || true
    git push --delete origin nightly || true
    gh release create nightly ./battle-instinct.zip ./battle-instinct_zh.zip -t "Nightly Build" -n "Nightly Build"