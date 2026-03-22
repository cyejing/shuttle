#!/bin/bash
set -e

CARGO_FILE="Cargo.toml"
BORER_CORE_CARGO="../borer-core/Cargo.toml"

current_version=$(grep -E '^version = "' "$CARGO_FILE" | head -1 | sed 's/version = "\(.*\)"/\1/')

major=$(echo "$current_version" | cut -d. -f1)
minor=$(echo "$current_version" | cut -d. -f2)
patch=$(echo "$current_version" | cut -d. -f3)

new_patch=$((patch + 1))
new_version="${major}.${minor}.${new_patch}"
tag_name="v$new_version"

echo "Shuttle Version: $current_version -> $new_version"
echo "Tag: $tag_name"

core_version=$(grep -E '^version = "' "$BORER_CORE_CARGO" | head -1 | sed 's/version = "\(.*\)"/\1/')

echo "Borer-core Version: $core_version"

if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/^version = \"$current_version\"/version = \"$new_version\"/" "$CARGO_FILE"
    sed -i '' "s/borer-core = { version=\"[^\"]*\"/borer-core = { version=\"$core_version\"/" "$CARGO_FILE"
else
    sed -i "s/^version = \"$current_version\"/version = \"$new_version\"/" "$CARGO_FILE"
    sed -i "s/borer-core = { version=\"[^\"]*\"/borer-core = { version=\"$core_version\"/" "$CARGO_FILE"
fi

if grep -q '^\[patch\.crates-io\]' "$CARGO_FILE"; then
    awk '
    /^\[patch\.crates-io\]/ { skip=1; next }
    skip && /^\[/ { skip=0 }
    !skip { print }
    ' "$CARGO_FILE" > "$CARGO_FILE.tmp" && mv "$CARGO_FILE.tmp" "$CARGO_FILE"
    echo "Removed [patch.crates-io] section"
fi

make check

git commit -a -m "release $tag_name"
git tag "$tag_name"
git push
git push origin "$tag_name"

echo "Done! Released $tag_name and pushed to remote."
