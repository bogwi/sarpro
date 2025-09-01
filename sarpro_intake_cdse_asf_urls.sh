# Change these to match your environment
export REMOTE_CACHE=/path/to/sarpro_cache
export OUT=/path/to/sarpro_output/urls_ASF_CDSE
export SARPRO=/path/to/sarpro/target/release/sarpro
export URLS_FILE=/path/to/urls.txt
mkdir -p "$REMOTE_CACHE" "$OUT"

process_url() {
  URL="$1"
  url_no_proto="${URL#https://}"
  domain="${url_no_proto%%/*}"
  output_dir="$OUT/$domain"
  mkdir -p "$output_dir"
  echo "Processing: $URL -> $output_dir"
  "$SARPRO" \
    --safe-zip-url "$URL" \
    --remote-cache-dir "$REMOTE_CACHE" \
    --output "$output_dir" \
    --format jpeg --polarization multiband --size 4096 --autoscale tamed \
    --target-crs auto \
    --log
}

export -f process_url

grep -v '^[[:space:]]*#' "$URLS_FILE" | grep -v '^[[:space:]]*$' | \
  xargs -I{} -P 4 bash -c 'process_url "$@"' _ {}