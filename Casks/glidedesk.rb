cask "glidedesk" do
  arch arm: "aarch64", intel: "universal"

  version "1.1.1"
  sha256 arm:   "bbd1103b6456a2db937c569ebebc8b6bfd30e38ed601b42e537cc7efa5a3d006",
         intel: "352b160a551551a973cbe66bcf4eef29984adfc0b2415ff04798cc1d5c6efa4e"

  url "https://github.com/glidedesk/GlideDesk/releases/download/v#{version}/GlideDesk_#{version}_#{arch}.dmg"
  name "GlideDesk"
  desc "Lightweight, zero-latency multi-device KVM & seamless workspace bridge"
  homepage "https://github.com/glidedesk/GlideDesk"

  auto_updates true
  depends_on macos: :ventura

  app "GlideDesk.app"

  uninstall quit: "com.glidedesk.desktop"

  zap trash: [
    "~/Library/Application Support/com.glidedesk.desktop",
    "~/Library/Preferences/com.glidedesk.desktop.plist",
    "~/Library/Saved Application State/com.glidedesk.desktop.savedState",
  ]
end
