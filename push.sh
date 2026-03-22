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

echo "Shuttle Version: $current_version -> $new_version"

core_current_version=$(grep -E '^version = "' "$BORER_CORE_CARGO" | head -1 | sed 's/version = "\(.*\)"/\1/')

core_major=$(echo "$core_current_version" | cut -d. -f1)
core_minor=$(echo "$core_current_version" | cut -d. -f2)
core_patch=$(echo "$core_current_version" | cut -d. -f3)

core_new_patch=$((core_patch + 1))
core_new_version="${core_major}.${core_minor}.${core_new_patch}"

echo "Borer-core Version: $core_current_version -> $core_new_version"

if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/^version = \"$current_version\"/version = \"$new_version\"/" "$CARGO_FILE"
    sed -i '' "s/^version = \"$core_current_version\"/version = \"$core_new_version\"/" "$BORER_CORE_CARGO"
    sed -i '' "s/borer-core = { version=\"$core_current_version\"/borer-core = { version=\"$core_new_version\"/" "$CARGO_FILE"
else
    sed -i "s/^version = \"$current_version\"/version = \"$new_version\"/" "$CARGO_FILE"
    sed -i "s/^version = \"$core_current_version\"/version = \"$core_new_version\"/" "$BORER_CORE_CARGO"
    sed -i "s/borer-core = { version=\"$core_current_version\"/borer-core = { version=\"$core_new_version\"/" "$CARGO_FILE"
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

git commit -a -m "bump version to $new_version"
git push

echo "Done! Version bumped to $new_version and pushed."
