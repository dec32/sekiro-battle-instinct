.\build -Release
git push --delete origin nightly
gh release create nightly ".\battle-instinct_x64.zip", ".\battle-instinct_zh_x64.zip" -t "Nightly Build" -n "Nightly Build"