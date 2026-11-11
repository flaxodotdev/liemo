# Releasing Liemo

Releases are built by GitHub Actions (`.github/workflows/release.yml`) when a version tag is
pushed. Installed copies of Liemo check `flaxodotdev/liemo`'s latest release at launch (at most
once a day, and from **Liemo → Check for Updates…**), and install it in place on one click: on
the Mac, Windows and Linux. Windows and Linux copies update themselves when they run from the
folder a release unpacked to (it has README.txt next to the program); copies installed some
other way are shown the release page instead.

## Cutting a release

1. Bump `version` in `native/Cargo.toml` (e.g. `0.2.0`) and commit.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The workflow builds Apple silicon and Intel apps, signs them with the Developer ID, notarises
   and staples them, builds Windows and Linux, signs every archive with the update key, and
   publishes the GitHub Release with
   generated notes. Edit the notes afterwards if you like: the app shows them in its update
   dialog.

Assets per release, which the updater looks for by name:

- `Liemo-<version>-macos-arm64.zip` and `.zip.sig`
- `Liemo-<version>-macos-x86_64.zip` and `.zip.sig`
- `Liemo-<version>-windows-x86_64.zip` and `.zip.sig`
- `Liemo-<version>-linux-x86_64.tar.gz` and `.tar.gz.sig`

On Windows and Linux the updater checks the signature and that the new program reports the
release's version (`--version`) before swapping it in. Linux swaps the folder and relaunches; on
Windows a hidden PowerShell step waits for Liemo to quit, copies the new files over and starts it.

Drafts and pre-releases (tags like `v0.3.0-beta.1`) are never offered to installed copies.

## Repository secrets

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | Your **Developer ID Application** certificate and key, exported from Keychain Access as .p12, base64: `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | The password you gave the .p12 export |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID1234)` (`security find-identity -v -p codesigning`) |
| `APPLE_ID` | The Apple ID email used for notarisation |
| `APPLE_TEAM_ID` | Your 10-character team id |
| `APPLE_APP_PASSWORD` | An app-specific password for that Apple ID (appleid.apple.com → Sign-In and Security) |
| `LIEMO_UPDATE_KEY` | The update signing key (below) |

## The update signing key

Every release zip is signed with an ed25519 key, and the app only installs a zip whose signature
matches the public key compiled into it (`PUBLIC_KEY` in `src/update.rs`). It also checks the new
app's bundle id, version and Apple code signature, and that it's signed by the same team as the
copy being replaced.

The key was generated with `liemo update-keygen <file>` and lives outside the repository.
Set the secret from it once:

```sh
gh secret set LIEMO_UPDATE_KEY --repo flaxodotdev/liemo < ~/.config/liemo/update-signing.key
```

**Keep a backup of that file somewhere safe.** If it's lost, existing installs can't verify new
releases: you'd ship a release with a new key that users install by hand once. If it leaks,
rotate it the same way. `liemo sign-update` refuses a key that doesn't match the built-in public
key, so CI can't publish a release that installed copies would reject.

## Building and testing locally

```sh
scripts/bundle.sh                       # dist/Liemo.app and the zip, ad-hoc signed
scripts/update-test.sh ~/.config/liemo/update-signing.key
```

`update-test.sh` installs the current version in a temporary folder, builds the next patch
version as a signed release, serves it through a local release feed (`LIEMO_UPDATE_FEED`) and
checks that the installed copy updates itself and relaunches. It runs muted, in the background,
with its own data folder.

Other switches: `LIEMO_NO_UPDATE_CHECK=1` skips the launch check;
`LIEMO_UPDATE_DIALOG=1` opens the update dialog at launch.

`scripts/make-icon.sh` rebuilds `assets/app/AppIcon.icns` from the mark.

## Notes

- FFmpeg comes from Homebrew on the build machine; `bundle.sh` copies it and its libraries into
  `Liemo.app/Contents/Frameworks` and points the app at them.
- `LSMinimumSystemVersion` is the macOS major version of the build machine, since Homebrew's
  libraries are built for it (macOS 15 on the current runners).
- Updates only install into a Liemo.app the user can write to (normally /Applications).
  Otherwise the dialog offers the release page instead.
