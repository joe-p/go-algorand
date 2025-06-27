#!/bin/bash

set -e

# Based on https://github.com/near/near-sdk-rs/blob/master/minifier/minify.sh

for p in "$@"; do
  w=$(basename -- $p)
  parent_directory=$(dirname -- `realpath $p`)
  out_path="$parent_directory/minified-$w"

  wasm-snip $p --snip-rust-fmt-code --snip-rust-panicking-code -p core::num::flt2dec::.* -p core::fmt::float::.*  \
     --output temp-$w
  wasm-strip temp-$w
  wasm-opt --enable-bulk-memory-opt -Oz temp-$w --output $out_path
  rm temp-$w
  echo $w `stat -c "%s" $p` "bytes ->" `stat -c "%s" $out_path` "bytes, see minified-$w"
done
