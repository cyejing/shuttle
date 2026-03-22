#!/bin/bash
set -e

CARGO_FILE="Cargo.toml"

if grep -q '^\[patch\.crates-io\]' "$CARGO_FILE"; then
    echo "[patch.crates-io] already exists, skipping..."
    exit 0
fi

awk '
/^\[dependencies\]/ {
    print "[patch.crates-io]"
    print "borer-core = { path = \"../borer/borer-core\" }"
    print ""
}
{ print }
' "$CARGO_FILE" > "$CARGO_FILE.tmp" && mv "$CARGO_FILE.tmp" "$CARGO_FILE"

echo "Added [patch.crates-io] section for local development."
