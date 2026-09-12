#!/bin/sh
set -eu

browser=
for candidate in google-chrome-stable google-chrome chrome chromium; do
  if command -v "$candidate" >/dev/null 2>&1; then
    browser="$(command -v "$candidate")"
    break
  fi
done

if [ -z "$browser" ]; then
  echo "No Chromium-family browser found" >&2
  exit 1
fi

if [ -z "${GITHUB_ENV:-}" ]; then
  echo "GITHUB_ENV is not set" >&2
  exit 1
fi
if [ -z "${RUNNER_TEMP:-}" ]; then
  echo "RUNNER_TEMP is not set" >&2
  exit 1
fi

mkdir -p "$RUNNER_TEMP/uf-browser"
wrapper="$RUNNER_TEMP/uf-browser/chromium"
cat > "$wrapper" <<'EOF'
#!/bin/sh
unset DBUS_SESSION_BUS_ADDRESS
exec "$UF_REAL_BROWSER" --no-sandbox --disable-setuid-sandbox --no-zygote "$@"
EOF
chmod +x "$wrapper"

echo "Using $browser for UF_BROWSER"
{
  echo "UF_REAL_BROWSER=$browser"
  echo "UF_BROWSER=$wrapper"
} >> "$GITHUB_ENV"
