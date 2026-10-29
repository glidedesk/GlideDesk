#!/usr/bin/env python3
"""
GlideDesk — Ultra-Clear Brandmark & Asset Generator (The G-Glide Monogram)
Generates:
- macOS .icns (native iconutil with retina scales)
- Windows .ico (multi-resolution 16, 24, 32, 48, 64, 128, 256)
- Linux / Web PNGs (16, 32, 48, 64, 128, 256, 512, 1024)
- In-App UI sidebar marks (@1x, @2x, @3x, @4x)
- Taskbar & Menu Bar Tray Glyphs (Light, Dark, macOS Template)
- Android App Icon & Hero Logos
"""
import os
import sys
import shutil
import subprocess
from PIL import Image, ImageChops, ImageDraw, ImageFilter

SS = 4
S = 1024 * SS
k = SS

def P(*v):
    return [round(x * k) for x in v]

def hexc(h, a=255):
    h = h.lstrip('#')
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4)) + (a,)

def solid(c, size=(S, S)):
    return Image.new('RGBA', size, c)

def rounded(box, r):
    m = Image.new('L', (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle(P(*box), radius=round(r * k), fill=255)
    return m

def paint(im, mask, col):
    layer = solid(col)
    layer.putalpha(mask.point(lambda v: v * col[3] // 255))
    return Image.alpha_composite(im, layer)

def render_g_glide_artwork():
    """Renders the bold, iconic, high-contrast G-Glide brandmark."""
    # 1. Dark Obsidian Canvas
    im = Image.new('RGBA', (S, S), hexc('#0b0f19'))

    # Rich ambient emerald glow behind the glyph
    glow = Image.new('L', (S, S), 0)
    ImageDraw.Draw(glow).ellipse(P(260, 140, 960, 780), fill=130)
    im = paint(im, glow.filter(ImageFilter.GaussianBlur(100 * k)), hexc('#10b981', 170))

    # Subtle cyan ambient fill at the lower left
    glow2 = Image.new('L', (S, S), 0)
    ImageDraw.Draw(glow2).ellipse(P(60, 480, 520, 960), fill=90)
    im = paint(im, glow2.filter(ImageFilter.GaussianBlur(90 * k)), hexc('#06b6d4', 130))

    # Outer squircle border for depth
    rim = ImageChops.subtract(rounded((0, 0, 1023, 1023), 224), rounded((6, 6, 1017, 1017), 218))
    im = paint(im, rim, hexc('#34d399', 55))

    # 2. Bold 'G' Ribbon
    # Outer circle R=365, Inner R=225 -> 140px thick solid stroke
    CX, CY = 512, 512
    R_OUT = 365
    R_IN = 225
    r_mid = (R_OUT + R_IN) / 2
    sw = (R_OUT - R_IN) * k

    mask = Image.new('L', (S, S), 0)
    d = ImageDraw.Draw(mask)

    # In PIL arc angles: 0=3 o'clock, 90=6 o'clock, 180=9 o'clock, 270=12 o'clock
    # G arc runs from 330 deg (top right) around bottom, left, top to 270 deg (top center)
    bbox_mid = P(CX - r_mid, CY - r_mid, CX + r_mid, CY + r_mid)
    d.arc(bbox_mid, start=330, end=270, fill=255, width=round(sw))

    # Top rounded cap (at 270 deg -> (CX, CY - r_mid))
    top_x, top_y = P(CX, CY - r_mid)
    d.ellipse([top_x - sw/2, top_y - sw/2, top_x + sw/2, top_y + sw/2], fill=255)

    # Horizontal crossbar of the G
    d.rounded_rectangle(P(CX - 50, CY - 70, CX + R_OUT - 15, CY + 70), radius=round(70 * k), fill=255)

    # Vibrant Mint -> Emerald -> Cyan vertical gradient
    grad = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    gd = ImageDraw.Draw(grad)
    for y in range(0, S, 4):
        t = y / S
        # Electric Mint (#34d399) to Emerald (#10b981) to Electric Cyan (#06b6d4)
        if t < 0.5:
            u = t * 2.0
            r = int(52 * (1 - u) + 16 * u)
            g = int(211 * (1 - u) + 185 * u)
            b = int(153 * (1 - u) + 129 * u)
        else:
            u = (t - 0.5) * 2.0
            r = int(16 * (1 - u) + 6 * u)
            g = int(185 * (1 - u) + 182 * u)
            b = int(129 * (1 - u) + 212 * u)
        gd.line([(0, y), (S, y)], fill=(r, g, b, 255), width=4)

    # Drop shadow behind the G
    sh = Image.new('L', (S, S), 0)
    sh.paste(mask, (0, round(22 * k)))
    im = paint(im, sh.filter(ImageFilter.GaussianBlur(30 * k)), (0, 0, 0, 255))

    # Composite G glyph
    g_layer = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    g_layer.paste(grad, (0, 0), mask)
    im = Image.alpha_composite(im, g_layer)

    # Delicate highlight edge on top of the G
    g_edge = ImageChops.subtract(mask, ImageChops.offset(mask, 0, round(5 * k)))
    im = paint(im, g_edge, (255, 255, 255, 120))

    # 3. Massive, Razor-Sharp White Cursor Arrow
    # Scaled to be unmistakable even at 16px
    arrow_pts = [
        (CX + 35, CY - 100),
        (CX + 35, CY + 155),
        (CX + 105, CY + 90),
        (CX + 160, CY + 205),
        (CX + 220, CY + 175),
        (CX + 162, CY + 65),
        (CX + 260, CY + 65)
    ]
    cur_pts = [P(x, y) for x, y in arrow_pts]

    # Deep cursor drop shadow
    c_sh = Image.new('L', (S, S), 0)
    ImageDraw.Draw(c_sh).polygon([(x + 10*k, y + 24*k) for x, y in cur_pts], fill=240)
    im = paint(im, c_sh.filter(ImageFilter.GaussianBlur(18 * k)), (0, 0, 0, 255))

    # White fill + bold obsidian border for punchy contrast
    cd = ImageDraw.Draw(im)
    cd.polygon(cur_pts, fill=(255, 255, 255, 255))
    cd.line(cur_pts + cur_pts[:2], fill=hexc('#0b0f19'), width=round(16 * k), joint='curve')

    # Internal emerald energy pip in the cursor center
    cd.ellipse(P(CX + 100, CY + 35, CX + 124, CY + 59), fill=hexc('#10b981'))

    return im

def frame_squircle(im, full_bleed=True):
    """Generates standard macOS squircle or full-bleed rounded icon."""
    out = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    if full_bleed:
        mask = rounded((0, 0, 1023, 1023), 224)
        out.paste(im, (0, 0), mask)
        return out.resize((1024, 1024), Image.LANCZOS)
    else:
        # macOS inset with soft natural drop shadow
        sh = Image.new('L', (S, S), 0)
        ImageDraw.Draw(sh).rounded_rectangle(P(100, 114, 924, 938), radius=round(185 * k), fill=130)
        out = paint(out, sh.filter(ImageFilter.GaussianBlur(16 * k)), (0, 0, 0, 255))

        layer = Image.new('RGBA', (S, S), (0, 0, 0, 0))
        layer.paste(im.resize((round(S * 824 / 1024),) * 2, Image.LANCZOS), (round(100 * k), round(100 * k)))
        out.paste(layer, (0, 0), rounded((100, 100, 924, 924), 185))
        return out.resize((1024, 1024), Image.LANCZOS)

def render_tray_glyph(size, color):
    """Renders single-color high-contrast vector G-Cursor for tray/menu bar."""
    glyph_s = size * 4
    canvas = Image.new('RGBA', (glyph_s, glyph_s), (0, 0, 0, 0))
    d = ImageDraw.Draw(canvas)

    cx, cy = glyph_s / 2, glyph_s / 2
    r_mid = glyph_s * 0.35
    sw = max(2, round(glyph_s * 0.16))

    bbox = [cx - r_mid, cy - r_mid, cx + r_mid, cy + r_mid]
    # In PIL: arc 330 deg to 270 deg
    d.arc(bbox, start=330, end=270, fill=color + (255,), width=sw)

    # Crossbar
    d.rounded_rectangle([cx - glyph_s*0.06, cy - sw/2, cx + r_mid + sw/2, cy + sw/2], radius=sw/2, fill=color + (255,))

    # Small sharp cursor arrow on top of the crossbar
    arrow_pts = [
        (cx + glyph_s*0.04, cy - glyph_s*0.14),
        (cx + glyph_s*0.04, cy + glyph_s*0.16),
        (cx + glyph_s*0.12, cy + glyph_s*0.08),
        (cx + glyph_s*0.18, cy + glyph_s*0.20),
        (cx + glyph_s*0.24, cy + glyph_s*0.17),
        (cx + glyph_s*0.18, cy + glyph_s*0.05),
        (cx + glyph_s*0.27, cy + glyph_s*0.05)
    ]
    # Cutout gap around cursor
    gap = Image.new('L', (glyph_s, glyph_s), 0)
    ImageDraw.Draw(gap).polygon(arrow_pts, fill=255)
    gap = gap.filter(ImageFilter.MaxFilter(round(sw * 0.8) | 1))
    canvas.paste((0, 0, 0, 0), (0, 0), gap)

    # Draw cursor arrow
    d.polygon(arrow_pts, fill=color + (255,))

    return canvas.resize((size, size), Image.LANCZOS)

def build_apple_icns(mac_img, out_icns_path):
    """Uses macOS native iconutil to generate a retina-grade .icns file."""
    if shutil.which('iconutil'):
        iconset_dir = out_icns_path.replace('.icns', '.iconset')
        os.makedirs(iconset_dir, exist_ok=True)
        scales = [
            (16, 'icon_16x16.png'),
            (32, 'icon_16x16@2x.png'),
            (32, 'icon_32x32.png'),
            (64, 'icon_32x32@2x.png'),
            (128, 'icon_128x128.png'),
            (256, 'icon_128x128@2x.png'),
            (256, 'icon_256x256.png'),
            (512, 'icon_256x256@2x.png'),
            (512, 'icon_512x512.png'),
            (1024, 'icon_512x512@2x.png'),
        ]
        for sz, fname in scales:
            mac_img.resize((sz, sz), Image.LANCZOS).save(os.path.join(iconset_dir, fname))
        subprocess.run(['iconutil', '-c', 'icns', iconset_dir, '-o', out_icns_path], check=True)
        shutil.rmtree(iconset_dir)
        print(f"  ✔ iconutil generated {out_icns_path}")
    else:
        mac_img.save(out_icns_path)
        print(f"  ✔ Pillow saved {out_icns_path}")

if __name__ == '__main__':
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.abspath(os.path.join(here, '..', '..', '..'))

    print("==> Rendering master G-Glide brandmark (4096px supersampled)...")
    art = render_g_glide_artwork()
    full = frame_squircle(art, full_bleed=True)
    mac = frame_squircle(art, full_bleed=False)

    # 1. Output to ui/shared/icon/
    print("==> Saving ui/shared/icon/ assets...")
    full.save(os.path.join(here, 'glidedesk.png'))
    mac.save(os.path.join(here, 'glidedesk-mac.png'))
    build_apple_icns(mac, os.path.join(here, 'glidedesk.icns'))
    full.save(os.path.join(here, 'glidedesk.ico'),
              sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    for n in (16, 32, 48, 64, 128, 256, 512):
        full.resize((n, n), Image.LANCZOS).save(os.path.join(here, f'glidedesk-{n}.png'))

    # 2. Output to src-tauri/icons/
    tauri_icons = os.path.join(root, 'src-tauri', 'icons')
    print("==> Updating src-tauri/icons/...")
    full.resize((32, 32), Image.LANCZOS).save(os.path.join(tauri_icons, '32x32.png'))
    full.resize((128, 128), Image.LANCZOS).save(os.path.join(tauri_icons, '128x128.png'))
    full.resize((256, 256), Image.LANCZOS).save(os.path.join(tauri_icons, '128x128@2x.png'))
    full.save(os.path.join(tauri_icons, 'icon.ico'),
              sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    build_apple_icns(mac, os.path.join(tauri_icons, 'icon.icns'))

    # 3. Output to ui/assets/
    ui_assets = os.path.join(root, 'ui', 'assets')
    print("==> Updating ui/assets/...")
    full.resize((256, 256), Image.LANCZOS).save(os.path.join(ui_assets, 'icon.png'))
    for scale, size in ((1, 60), (2, 120), (3, 180), (4, 240)):
        full.resize((size, size), Image.LANCZOS).save(os.path.join(ui_assets, f'sidebar-icon@{scale}x.png'))

    # 4. Tray icons for Windows, Linux, and macOS
    print("==> Updating tray glyphs...")
    win_assets = os.path.join(root, 'windows', 'assets')
    lin_assets = os.path.join(root, 'linux', 'assets')
    os.makedirs(win_assets, exist_ok=True)
    os.makedirs(lin_assets, exist_ok=True)

    WHITE = (255, 255, 255)
    BLACK = (0, 0, 0)

    # Windows taskbar
    for base, col in (('tray-white', WHITE), ('tray-black', BLACK), ('tray', WHITE)):
        render_tray_glyph(16, col).save(os.path.join(win_assets, f'{base}.png'))
        render_tray_glyph(32, col).save(os.path.join(win_assets, f'{base}@2x.png'))
        render_tray_glyph(16, col).save(os.path.join(ui_assets, f'{base}.png'))
        render_tray_glyph(32, col).save(os.path.join(ui_assets, f'{base}@2x.png'))

    # Linux panel
    render_tray_glyph(16, WHITE).save(os.path.join(lin_assets, 'tray.png'))
    render_tray_glyph(32, WHITE).save(os.path.join(lin_assets, 'tray@2x.png'))

    # macOS menu bar template
    render_tray_glyph(18, BLACK).save(os.path.join(here, 'trayTemplate.png'))
    render_tray_glyph(36, BLACK).save(os.path.join(here, 'trayTemplate@2x.png'))
    render_tray_glyph(18, BLACK).save(os.path.join(ui_assets, 'trayTemplate.png'))
    render_tray_glyph(36, BLACK).save(os.path.join(ui_assets, 'trayTemplate@2x.png'))

    # 5. Android app icons
    android_res = os.path.join(root, 'android', 'app', 'src', 'main', 'res')
    if os.path.exists(android_res):
        print("==> Updating Android app launcher icons & logo...")
        full.resize((240, 240), Image.LANCZOS).save(os.path.join(android_res, 'drawable-nodpi', 'glidedesk_logo.png'))
        full.resize((48, 48), Image.LANCZOS).save(os.path.join(android_res, 'mipmap-mdpi', 'ic_launcher.png'))
        full.resize((72, 72), Image.LANCZOS).save(os.path.join(android_res, 'mipmap-hdpi', 'ic_launcher.png'))
        full.resize((96, 96), Image.LANCZOS).save(os.path.join(android_res, 'mipmap-xhdpi', 'ic_launcher.png'))
        full.resize((144, 144), Image.LANCZOS).save(os.path.join(android_res, 'mipmap-xxhdpi', 'ic_launcher.png'))
        full.resize((192, 192), Image.LANCZOS).save(os.path.join(android_res, 'mipmap-xxxhdpi', 'ic_launcher.png'))

    print("==> The G-Glide brand assets successfully generated across all platforms!")
