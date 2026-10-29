# GlideDesk on NixOS & Nix

GlideDesk provides first-class support for NixOS and Nix via `flake.nix`.

---

## 🚀 Quick Run with Nix Flakes

You can run GlideDesk directly without installing it system-wide:

```sh
nix run github:glidedesk/glidedesk
# Or from a local clone:
nix run .
```

To enter a development environment with all required C libraries, Node.js, Electron, and Python GStreamer typelibs:

```sh
nix develop
```

---

## ❄️ NixOS System Configuration

Add GlideDesk to your system flake inputs and enable the NixOS module:

### `flake.nix`
```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    glidedesk.url = "github:glidedesk/glidedesk"; # or local checkout
  };

  outputs = { self, nixpkgs, glidedesk, ... }: {
    nixosConfigurations.myhostname = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        glidedesk.nixosModules.default
        {
          # Enable GlideDesk
          programs.glidedesk = {
            enable = true;
            user = "yourusername"; # Automatically adds your user to 'input' and 'uinput' groups
          };
        }
      ];
    };
  };
}
```

> **Note:** Both `programs.glidedesk` and `services.glidedesk` are accepted by the module.

---

## ⚙️ Module Options

| Option | Type | Default | Description |
|---|---|---|---|
| `programs.glidedesk.enable` | `bool` | `false` | Enable GlideDesk application and system integration. |
| `programs.glidedesk.package` | `package` | `glidedesk` | The GlideDesk package derivation to use. |
| `programs.glidedesk.user` | `nullOr str` | `null` | Your username. If specified, adds the user to the `input` and `uinput` groups. |
| `programs.glidedesk.openFirewall` | `bool` | `true` | Automatically opens TCP port `47800` (peer traffic) and UDP ports `47801` (discovery) & `5353` (mDNS). |
| `programs.glidedesk.uinput` | `bool` | `true` | Enables `hardware.uinput.enable` and configures udev rules for Wayland input capture & injection. |
| `programs.glidedesk.enableWifiDirect` | `bool` | `true` | Creates `/run/wrappers/bin/GlideDesk-wifiap` with `CAP_NET_ADMIN` so Wi-Fi Direct interface management works seamlessly without root password prompts. |

---

## 🛠️ Features & How They Work on NixOS

### 1. Wayland Mouse & Keyboard (KVM)
On Wayland, input capture and injection operate via `/dev/input/event*` and `/dev/uinput`.
- The NixOS module automatically enables `hardware.uinput.enable = true;` and configures `/etc/udev/rules.d` for uinput.
- Ensure your user belongs to both `input` and `uinput` groups (configured automatically when setting `programs.glidedesk.user = "yourusername";`).
- Remember to log out and back in if you were just added to those groups.

### 2. Wi-Fi Direct & Hotspot Links
Creating a secondary access point interface on supported Wi-Fi adapters requires `CAP_NET_ADMIN`.
- The NixOS module automatically configures a security capability wrapper:
  ```nix
  security.wrappers.GlideDesk-wifiap = {
    source = "${pkg}/libexec/GlideDesk-wifiap";
    capabilities = "cap_net_admin+ep";
    owner = "root";
    group = "root";
  };
  ```
- GlideDesk automatically detects `/run/wrappers/bin/GlideDesk-wifiap` and connects without needing `pkexec` prompts.

### 3. Extend (Virtual Displays)
- **GNOME on Wayland**: Uses Mutter's virtual monitor portal. The package wrapper automatically bundles and sets up Python 3 with `pygobject` and `gst-python` with all GStreamer PipeWire plugins and GI typelibs.
- **X11**: The module automatically installs the Polkit policy (`org.glidedesk.linux.connector.policy`) and connector script (`GlideDesk-connector`) into `/run/current-system/sw` so output switching works out of the box.

### 4. File Sharing & Remote Terminal
- **OpenSSH (Terminal)**: Enable `services.openssh.enable = true;` in your NixOS configuration.
- **Samba (Folder Sharing)**: Enable `services.samba.enable = true;` in your NixOS configuration.
