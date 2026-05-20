#!/usr/bin/env python3
from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "assets" / "exam-guard-icon.png"
ICON_DIRS = [
    ROOT / "apps" / "client" / "src-tauri" / "icons",
    ROOT / "apps" / "server" / "src-tauri" / "icons",
]


def rounded_rectangle_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def make_source_icon() -> Image.Image:
    size = 1024
    icon = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    background = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(background)
    draw.rounded_rectangle((0, 0, size, size), radius=228, fill=(16, 28, 45, 255))

    for y in range(size):
        tint = int(34 * y / size)
        draw.line((0, y, size, y), fill=(16 + tint, 28 + tint, 45 + tint, 255))

    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    glow_draw = ImageDraw.Draw(glow)
    glow_draw.ellipse((-180, -120, 670, 700), fill=(34, 197, 94, 72))
    glow_draw.ellipse((420, 240, 1180, 1120), fill=(37, 99, 235, 78))
    glow = glow.filter(ImageFilter.GaussianBlur(48))
    background.alpha_composite(glow)

    icon.alpha_composite(background)
    draw = ImageDraw.Draw(icon)

    shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    shadow_draw = ImageDraw.Draw(shadow)
    shield = [(512, 148), (800, 264), (754, 714), (512, 878), (270, 714), (224, 264)]
    shadow_draw.polygon([(x, y + 22) for x, y in shield], fill=(0, 0, 0, 78))
    shadow = shadow.filter(ImageFilter.GaussianBlur(24))
    icon.alpha_composite(shadow)

    draw.polygon(shield, fill=(241, 245, 249, 255))
    inner = [(512, 205), (742, 298), (706, 678), (512, 810), (318, 678), (282, 298)]
    draw.polygon(inner, fill=(37, 99, 235, 255))

    screen = (334, 372, 690, 594)
    draw.rounded_rectangle(screen, radius=42, fill=(15, 23, 42, 255))
    draw.rounded_rectangle((368, 408, 656, 550), radius=22, fill=(96, 165, 250, 255))
    draw.rectangle((480, 594, 544, 648), fill=(15, 23, 42, 255))
    draw.rounded_rectangle((418, 642, 606, 682), radius=20, fill=(15, 23, 42, 255))

    eye_shadow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    eye_shadow_draw = ImageDraw.Draw(eye_shadow)
    eye_shadow_draw.ellipse((404, 414, 620, 550), fill=(0, 0, 0, 68))
    eye_shadow = eye_shadow.filter(ImageFilter.GaussianBlur(10))
    icon.alpha_composite(eye_shadow)

    draw = ImageDraw.Draw(icon)
    draw.ellipse((400, 400, 624, 550), fill=(248, 250, 252, 255))
    draw.ellipse((452, 418, 572, 538), fill=(30, 64, 175, 255))
    draw.ellipse((488, 454, 536, 502), fill=(15, 23, 42, 255))
    draw.ellipse((516, 434, 548, 466), fill=(255, 255, 255, 230))

    draw.rounded_rectangle((640, 674, 774, 802), radius=36, fill=(34, 197, 94, 255))
    draw.line((674, 738, 708, 772, 746, 704), fill=(255, 255, 255, 255), width=24)

    icon.putalpha(rounded_rectangle_mask(size, 228))
    return icon


def write_iconset(icon: Image.Image, icon_dir: Path) -> None:
    icon_dir.mkdir(parents=True, exist_ok=True)
    icon.save(icon_dir / "icon.png")

    for name, size in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon-256.png", 256),
        ("icon-512.png", 512),
    ]:
        icon.resize((size, size), Image.Resampling.LANCZOS).save(icon_dir / name)

    icon.save(
        icon_dir / "icon.ico",
        sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )

    if shutil.which("iconutil"):
        iconset = icon_dir / "icon.iconset"
        if iconset.exists():
            shutil.rmtree(iconset)
        iconset.mkdir()
        for filename, size in [
            ("icon_16x16.png", 16),
            ("icon_16x16@2x.png", 32),
            ("icon_32x32.png", 32),
            ("icon_32x32@2x.png", 64),
            ("icon_128x128.png", 128),
            ("icon_128x128@2x.png", 256),
            ("icon_256x256.png", 256),
            ("icon_256x256@2x.png", 512),
            ("icon_512x512.png", 512),
            ("icon_512x512@2x.png", 1024),
        ]:
            icon.resize((size, size), Image.Resampling.LANCZOS).convert("RGB").save(
                iconset / filename
            )

        result = subprocess.run(
            ["iconutil", "-c", "icns", "-o", str(icon_dir / "icon.icns"), str(iconset)],
            check=False,
        )
        shutil.rmtree(iconset)
        if result.returncode != 0:
            print(f"warning: iconutil could not create {icon_dir / 'icon.icns'}")


def main() -> None:
    icon = make_source_icon()
    SOURCE.parent.mkdir(parents=True, exist_ok=True)
    icon.save(SOURCE)

    for icon_dir in ICON_DIRS:
        write_iconset(icon, icon_dir)


if __name__ == "__main__":
    main()
