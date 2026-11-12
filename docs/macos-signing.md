# Signing GlideDesk on macOS

macOS ties privacy grants (Screen Recording, Accessibility, Full Disk Access)
to the app's code requirement. An ad-hoc signature (`codesign --sign -`, which
is what a plain `cargo build`/`tauri build` produces) changes on every build, so
the system keeps asking for the same permission and ScreenCaptureKit refuses to
start with "user declined" even while System Settings shows the switch on.

Sign every build that is going to be used for more than a smoke test with one
stable identity.

## One-time identity

A self-signed code signing certificate is enough; it does not need to be issued
by Apple.

```sh
security create-keychain -p glidedesk ~/Library/Keychains/glidedesk-signing.keychain-db
security set-keychain-settings -lut 21600 ~/Library/Keychains/glidedesk-signing.keychain-db
security unlock-keychain -p glidedesk ~/Library/Keychains/glidedesk-signing.keychain-db

# Create "GlideDesk Dev" (Code Signing) in Keychain Access, then trust it for
# code signing (Keychain Access > certificate > Trust > Code Signing: Always Trust),
# or copy an existing certificate and key into the keychain.
security find-identity -v -p codesigning
```

Add the keychain to the search list so `codesign` finds the identity:

```sh
security list-keychains -d user -s "$(security default-keychain -d user | tr -d '"')" ~/Library/Keychains/glidedesk-signing.keychain-db
```

## Signing a build

```sh
cargo build --release            # or: cargo tauri build
security unlock-keychain -p glidedesk ~/Library/Keychains/glidedesk-signing.keychain-db
codesign --force --deep --sign 3AAA1DA4135930B21D9582D284022B5031996887 \
  --keychain ~/Library/Keychains/glidedesk-signing.keychain-db \
  /Applications/GlideDesk.app
```

`codesign -d -r- /Applications/GlideDesk.app` should print the same
`certificate root = H"3aaa1da4…"` after every rebuild.

## Usage descriptions

`src-tauri/Info.plist` carries `NSScreenCaptureUsageDescription`,
`NSMicrophoneUsageDescription`, `NSAudioCaptureUsageDescription` and
`NSCameraUsageDescription`. Tauri merges it into the bundle. Without the screen
capture string macOS will not even show the request; `CGRequestScreenCaptureAccess`
returns false immediately and no row appears in TCC.

## After an upgrade

A grant made against the stable identity survives rebuilds. If an older ad-hoc
build was granted once, reset the stale row and approve the signed build again:

```sh
tccutil reset ScreenCapture com.glidedesk.desktop
open /Applications/GlideDesk.app
```

The Audio log line `[Audio] Screen Recording permission: true` confirms the
effective grant; the System Settings switch alone is not proof.
