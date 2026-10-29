cask "glidedesk" do
  arch arm: "aarch64", intel: "x64"

  version "1.1.1"
  sha256 arm:   "bbd1103b6456a2db937c569ebebc8b6bfd30e38ed601b42e537cc7efa5a3d006",
         intel: "bbd1103b6456a2db937c569ebebc8b6bfd30e38ed601b42e537cc7efa5a3d006"

  url "https://github.com/glidedesk/glidedesk/releases/download/v#{version}/GlideDesk_#{version}_#{arch}.dmg"
  name "GlideDesk"
  desc "Lightweight, zero-latency multi-device KVM & seamless workspace bridge"
  homepage "https://github.com/glidedesk/glidedesk"

  auto_updates true
  depends_on macos: ">= :ventura"

  app "GlideDesk.app"

  preflight do
    # When updating a running instance on a headless or mouse-less Mac, smoothly close it
    # without failing the Homebrew upgrade process
    was_running = system_command("/usr/bin/pgrep", args: ["-ix", "glidedesk"], result_type: :exit_status).zero?
    File.write("#{staged_path}/.was_running", "1") if was_running
    system_command "/usr/bin/pkill", args: ["-ix", "glidedesk"], sudo: false if was_running
  end

  postflight do
    # 1. Clear quarantine extended attribute to prevent macOS Gatekeeper "Move to Bin" warning
    system_command "/usr/bin/xattr",
                   args: ["-cr", "#{appdir}/GlideDesk.app"],
                   sudo: false

    # 2. If GlideDesk was active before upgrading, relaunch it immediately so remote KVM
    # mouse and keyboard control is instantly restored without stranding the user
    if File.exist?("#{staged_path}/.was_running")
      system_command "/usr/bin/open", args: ["-a", "#{appdir}/GlideDesk.app"], sudo: false
    end
  end

  uninstall quit: "com.glidedesk.desktop"

  zap trash: [
    "~/Library/Application Support/com.glidedesk.desktop",
    "~/Library/Preferences/com.glidedesk.desktop.plist",
    "~/Library/Saved Application State/com.glidedesk.desktop.savedState",
  ]
end
