# Kimi Desktop for Linux

[![GitHub release (latest by date)](https://img.shields.io/github/v/release/bigjoe420/kimi-desktop-linux?style=flat-square)](https://github.com/bigjoe420/kimi-desktop-linux/releases)

A minimal, native desktop wrapper for [Kimi](https://kimi.ai) built with [Tauri v2](https://tauri.app).

---

## Motivation

The official Kimi Linux package is broken on modern Ubuntu LTS releases. It links against `libwebkit2gtk-4.0`, which was removed from Ubuntu 22.04+ and Debian 12+ repositories in favor of `libwebkit2gtk-4.1`. As a result, the official client fails to install or run on:

- Ubuntu 22.04 LTS / 24.04 LTS
- Debian 12 (Bookworm)
- Any distribution that no longer ships `libwebkit2gtk-4.0`

This wrapper solves the problem by building on Tauri v2, which links against `libwebkit2gtk-4.1` — the actively maintained WebKitGTK branch. The result is a single native binary with a system webview, no bundled Chromium, and full desktop integration, installable on current distributions with no workarounds.

![Kimi Desktop Linux running on Ubuntu](assets/screenshot.png)

---

## Disclaimer

Noad Laboratories is an independent entity and is not affiliated with, endorsed by, sponsored by, or otherwise connected to Moonshot AI. "Kimi" is a product of Moonshot AI; this project is an unofficial, community-built desktop wrapper and uses no Moonshot AI branding or assets. All trademarks belong to their respective owners.

---

## Features

- Native WebKit2GTK-4.1 webview
- Persistent window state (size/position remembered across sessions)
- Single-instance enforcement (relaunching focuses the existing window)
- External links open in the system browser
- Sign in with Google works inside the app (in-app popup that closes itself when login completes)
- SSO / "open in browser" requests from the Kimi site are handled natively
- `.deb` and AppImage bundles
- Minimal codebase, no framework bloat

---

## Signing in

Phone number, Google and SSO logins are supported.

- **Google:** the Google sign-in opens in a small in-app popup (500x640). WebKitGTK ignores `window.open()` by default, so the app handles popup requests itself and keeps the popup connected to the main window so the login result gets back to Kimi. When the popup returns to Kimi it closes automatically and the main window reloads to pick up your session.
- **SSO and external links:** the Kimi site calls a native `open_external` command, which opens the link in your default browser. Only `http`, `https` and `mailto` links are allowed. Links to unrelated sites also open in the system browser.
- **Separate browser profile:** the app is its own browser with its own cookies, so Google and Kimi list it as a separate device in account security, and Google may show a one-time "new sign-in" notice. Your session is stored locally and persists between launches.
- **Reset the session:** delete `~/.local/share/cn.moonshot.kimi.desktop` to clear cookies and local data.

---

## Installation

### Prebuilt packages (recommended)

Download the latest `.deb` or `.AppImage` from the [GitHub Releases](https://github.com/bigjoe420/kimi-desktop-linux/releases) page.

**Debian/Ubuntu (.deb):**

```bash
sudo apt install ./Kimi_0.1.0_amd64.deb
```

`apt` resolves the `libwebkit2gtk-4.1-0` dependency automatically. Launch Kimi from your application menu afterward.

**AppImage (portable):**

```bash
chmod +x Kimi_0.1.0_amd64.AppImage
./Kimi_0.1.0_amd64.AppImage
```

### Build from source

| Distro | Minimum Version |
|--------|----------------|
| Ubuntu | 22.04 LTS |
| Debian | 12 (Bookworm) |

```bash
git clone https://github.com/bigjoe420/kimi-desktop-linux.git
cd kimi-desktop-linux
chmod +x build.sh
./build.sh
```

The script will:

1. Install system dependencies via `apt`
2. Install Rust (if missing)
3. Install the Tauri v2 CLI (pinned to `2.11.4`)
4. Build release bundles

Output artifacts:

- `src-tauri/target/release/bundle/deb/*.deb`
- `src-tauri/target/release/bundle/appimage/*.AppImage`

### Manual build

If you prefer not to use the script:

```bash
# 1. Install dependencies
sudo apt-get update
sudo apt-get install -y build-essential curl wget libssl-dev libgtk-3-dev \
  libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev patchelf pkg-config

# 2. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 3. Install Tauri CLI
cargo install tauri-cli --version "2.11.4" --locked

# 4. Build
cargo tauri build
```

---

## Project Structure

```
kimi-desktop-linux/
├── src/
│   └── index.html              # Splash loader shown while Kimi loads
├── src-tauri/
│   ├── Cargo.toml              # Rust dependencies
│   ├── tauri.conf.json         # App & window configuration (main window is created in code)
│   ├── build.rs                # Build script, registers the `open_external` command
│   ├── capabilities/
│   │   └── default.json        # Runtime permissions, including the remote kimi.ai / kimi.com origins
│   ├── icons/                  # Noad Labs branding assets
│   └── src/
│       ├── main.rs             # Entry point
│       └── lib.rs              # App bootstrap, popup handling, `open_external` command
├── build.sh                    # One-command build script
└── README.md
```

---

## Development

```bash
cargo tauri dev
```

The first build compiles several hundred crates and can take a while; later builds are incremental. Debug builds open the web inspector automatically, and the terminal prints `[kimi-desktop-linux]` log lines for popup requests and page loads, which is useful when debugging login flows.

---

## License

MIT — see [LICENSE](LICENSE).

---

*Maintained by Joe Kyser (Noad Laboratories)*
