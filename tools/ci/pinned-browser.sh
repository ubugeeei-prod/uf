#!/bin/sh
set -eu

# Chrome for Testing, from Google's versioned distribution. The screenshot
# gate uses the same binary on every runner, independently of the base image.
version=153.0.8010.52
: "${RUNNER_TEMP:?RUNNER_TEMP must name the CI scratch directory}"
: "${GITHUB_ENV:?GITHUB_ENV must name the Actions environment file}"
directory="$RUNNER_TEMP/uf-chrome-$version"
mkdir -p "$directory"
curl --fail --location --retry 3 --silent --show-error \
  "https://storage.googleapis.com/chrome-for-testing-public/$version/linux64/chrome-linux64.zip" \
  --output "$directory/chrome.zip"
unzip -q "$directory/chrome.zip" -d "$directory"
wrapper="$directory/browser"
cat > "$wrapper" <<'EOF'
#!/bin/sh
unset DBUS_SESSION_BUS_ADDRESS
exec "$(dirname "$0")/chrome-linux64/chrome" --no-sandbox --disable-setuid-sandbox "$@"
EOF
chmod +x "$wrapper"
"$wrapper" --version
echo "UF_BROWSER=$wrapper" >> "$GITHUB_ENV"
