cask "glidedesk" do
  arch arm: "aarch64", intel: "x64"

  version "1.0.0"
  sha256 arm:   "d4b4adc5b6f036822fae9ae46ef15234a72240883b9967833216f523afb77b18"

  url "https://github.com/benzntech/TwinFlow/releases/download/v#{version}/GlideDesk_#{version}_#{arch}.dmg"
  name "GlideDesk"
  desc "Lightweight, zero-latency multi-device KVM & seamless workspace bridge"
  homepage "https://github.com/benzntech/TwinFlow"

  auto_updates true
  depends_on macos: ">= :ventura"

  app "GlideDesk.app"

  preflight do
    # When updating a running instance on a headless or mouse-less Mac, smoothly close it
    # without failing the Homebrew upgrade process
    was_running = system_command("/usr/bin/pgrep", args: ["-x", "GlideDesk"], result_type: :exit_status).zero?
    File.write("#{staged_path}/.was_running", "1") if was_running
    system_command "/usr/bin/pkill", args: ["-x", "GlideDesk"], sudo: false if was_running
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

  uninstall quit: "com.glidedesk.app"

  zap trash: [
    "~/Library/Application Support/com.glidedesk.app",
    "~/Library/Preferences/com.glidedesk.app.plist",
    "~/Library/Saved Application State/com.glidedesk.app.savedState",
  ]
end
