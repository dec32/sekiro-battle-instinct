.\build -Release
git push --delete origin nightly
gh release create nightly ".\battle-instinct.zip", ".\battle-instinct_zh.zip" -t "Nightly Build" -n "Nightly Build"