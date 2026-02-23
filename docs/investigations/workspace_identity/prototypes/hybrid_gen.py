#!/usr/bin/env python3
"""
Prototype: Hybrid approach - identicon pattern as tile background with
the status indicator overlaid. Tests whether the identicon works as a
recognizable "avatar" within the existing tile layout.

Also tests a smaller identicon (3x3 grid) which may be more readable
at 48px than 5x5.
"""

import hashlib
import colorsys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont


def identicon_data(name: str, grid_size: int = 5):
    """Generate identicon parameters from a workspace name."""
    h = hashlib.sha256(name.encode('utf-8')).digest()

    hue = (h[0] | (h[1] << 8)) % 360
    sat = 0.5 + (h[2] / 255.0) * 0.3
    light = 0.4 + (h[3] / 255.0) * 0.25

    r, g, b = colorsys.hls_to_rgb(hue / 360.0, light, sat)
    fg_color = (int(r * 255), int(g * 255), int(b * 255))

    half = (grid_size + 1) // 2
    bits_needed = grid_size * half
    bits = int.from_bytes(h[4:8], 'little')

    grid = [[False] * grid_size for _ in range(grid_size)]
    for row in range(grid_size):
        for col in range(half):
            bit_index = row * half + col
            on = bool(bits & (1 << bit_index))
            grid[row][col] = on
            grid[row][grid_size - 1 - col] = on

    return fg_color, grid


def draw_tile(draw, x, y, tile_w, tile_h, name, grid_size=5, show_status=True, scale=1):
    """Draw a complete tile with identicon, status dot, and label."""
    fg_color, grid = identicon_data(name, grid_size)
    bg_color = (30, 30, 36)

    # Tile background
    draw.rectangle([x, y, x + tile_w - 1, y + tile_h - 1], fill=bg_color)

    # Identicon - fill most of the tile
    padding = 4 * scale
    icon_area_w = tile_w - 2 * padding
    icon_area_h = tile_h - 2 * padding
    cell_w = icon_area_w // grid_size
    cell_h = icon_area_h // grid_size

    gx = x + (tile_w - cell_w * grid_size) // 2
    gy = y + (tile_h - cell_h * grid_size) // 2

    # Draw dimmed background cells for "off" cells (subtle grid)
    dim_color = tuple(c // 5 for c in fg_color)
    for row in range(grid_size):
        for col in range(grid_size):
            cx = gx + col * cell_w
            cy = gy + row * cell_h
            if grid[row][col]:
                draw.rectangle([cx, cy, cx + cell_w - 1, cy + cell_h - 1], fill=fg_color)
            else:
                draw.rectangle([cx, cy, cx + cell_w - 1, cy + cell_h - 1], fill=dim_color)

    # Status indicator dot (top-right corner)
    if show_status:
        dot_size = 6 * scale
        dot_x = x + tile_w - dot_size - 2 * scale
        dot_y = y + 2 * scale
        # Green "running" status
        draw.ellipse([dot_x, dot_y, dot_x + dot_size, dot_y + dot_size],
                     fill=(50, 200, 50))


def main():
    workspace_names = [
        "project-alpha", "project-beta", "project-gamma", "project-delta",
        "main", "feature/auth", "feature/ui", "bugfix/crash",
        "untitled", "untitled-2", "workspace-1", "workspace-2",
    ]

    cols = 4
    rows = 3
    padding = 12

    # ---- Sheet 1: 5x5 identicons at actual tile size (48x48) ----
    tile_w, tile_h = 48, 48
    label_h = 14
    sheet_w = cols * (tile_w + padding) + padding
    sheet_h = rows * (tile_h + label_h + padding) + padding + 20

    img = Image.new('RGB', (sheet_w, sheet_h), color=(20, 20, 24))
    draw = ImageDraw.Draw(img)

    try:
        font = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 9)
    except:
        font = ImageFont.load_default()

    # Title
    draw.text((padding, 4), "5x5 grid @ 48px", fill=(150, 150, 160), font=font)

    for i, name in enumerate(workspace_names):
        col = i % cols
        row = i // cols
        tx = padding + col * (tile_w + padding)
        ty = 20 + row * (tile_h + label_h + padding)
        draw_tile(draw, tx, ty, tile_w, tile_h, name, grid_size=5)

        display = name if len(name) <= 14 else name[:12] + ".."
        bbox = draw.textbbox((0, 0), display, font=font)
        tw = bbox[2] - bbox[0]
        draw.text((tx + (tile_w - tw) // 2, ty + tile_h + 1),
                  display, fill=(140, 140, 150), font=font)

    out1 = Path(__file__).parent / "hybrid_5x5_48px.png"
    img.save(str(out1))
    print(f"Saved: {out1}")

    # ---- Sheet 2: 3x3 identicons at actual tile size ----
    img2 = Image.new('RGB', (sheet_w, sheet_h), color=(20, 20, 24))
    draw2 = ImageDraw.Draw(img2)
    draw2.text((padding, 4), "3x3 grid @ 48px", fill=(150, 150, 160), font=font)

    for i, name in enumerate(workspace_names):
        col = i % cols
        row = i // cols
        tx = padding + col * (tile_w + padding)
        ty = 20 + row * (tile_h + label_h + padding)
        draw_tile(draw2, tx, ty, tile_w, tile_h, name, grid_size=3)

        display = name if len(name) <= 14 else name[:12] + ".."
        bbox = draw2.textbbox((0, 0), display, font=font)
        tw = bbox[2] - bbox[0]
        draw2.text((tx + (tile_w - tw) // 2, ty + tile_h + 1),
                   display, fill=(140, 140, 150), font=font)

    out2 = Path(__file__).parent / "hybrid_3x3_48px.png"
    img2.save(str(out2))
    print(f"Saved: {out2}")

    # ---- Sheet 3: 2x scale for detail inspection ----
    scale = 2
    tile_w2, tile_h2 = 48 * scale, 48 * scale
    label_h2 = 16
    sheet_w2 = cols * (tile_w2 + padding) + padding
    sheet_h2 = rows * (tile_h2 + label_h2 + padding) + padding + 24

    img3 = Image.new('RGB', (sheet_w2, sheet_h2), color=(20, 20, 24))
    draw3 = ImageDraw.Draw(img3)

    try:
        font2 = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 11)
    except:
        font2 = ImageFont.load_default()

    draw3.text((padding, 4), "5x5 grid @ 96px (2x zoom for inspection)", fill=(150, 150, 160), font=font2)

    for i, name in enumerate(workspace_names):
        col = i % cols
        row = i // cols
        tx = padding + col * (tile_w2 + padding)
        ty = 24 + row * (tile_h2 + label_h2 + padding)
        draw_tile(draw3, tx, ty, tile_w2, tile_h2, name, grid_size=5, scale=scale)

        bbox = draw3.textbbox((0, 0), name, font=font2)
        tw = bbox[2] - bbox[0]
        draw3.text((tx + (tile_w2 - tw) // 2, ty + tile_h2 + 1),
                   name, fill=(140, 140, 150), font=font2)

    out3 = Path(__file__).parent / "hybrid_5x5_96px.png"
    img3.save(str(out3))
    print(f"Saved: {out3}")

    # ---- Sheet 4: Simulated left rail (vertical strip) ----
    rail_w = 56
    rail_tile_h = 48
    rail_spacing = 4
    rail_top = 8
    n_visible = min(8, len(workspace_names))

    rail_h = rail_top + n_visible * (rail_tile_h + rail_spacing)
    # Put 1x on left, 2x on right for comparison
    img4 = Image.new('RGB', (rail_w + 40 + rail_w * 2, rail_h * 2), color=(20, 20, 24))
    draw4 = ImageDraw.Draw(img4)

    # Rail background
    draw4.rectangle([0, 0, rail_w - 1, rail_h * 2 - 1], fill=(30, 30, 36))

    for i in range(n_visible):
        name = workspace_names[i]
        tile_x = 4
        tile_y = rail_top + i * (rail_tile_h + rail_spacing)
        draw_tile(draw4, tile_x, tile_y, rail_w - 8, rail_tile_h, name, grid_size=5)

        # Active indicator for first tile
        if i == 0:
            draw4.rectangle([0, tile_y, 2, tile_y + rail_tile_h - 1], fill=(100, 140, 255))

    # 2x version on the right
    x_off = rail_w + 40
    draw4.rectangle([x_off, 0, x_off + rail_w * 2 - 1, rail_h * 2 - 1], fill=(30, 30, 36))

    for i in range(n_visible):
        name = workspace_names[i]
        tile_x = x_off + 8
        tile_y = rail_top * 2 + i * ((rail_tile_h + rail_spacing) * 2)
        if tile_y + rail_tile_h * 2 > rail_h * 2:
            break
        draw_tile(draw4, tile_x, tile_y, (rail_w - 8) * 2, rail_tile_h * 2, name, grid_size=5, scale=2)

        if i == 0:
            draw4.rectangle([x_off, tile_y, x_off + 4, tile_y + rail_tile_h * 2 - 1],
                           fill=(100, 140, 255))

    out4 = Path(__file__).parent / "simulated_rail.png"
    img4.save(str(out4))
    print(f"Saved: {out4}")


if __name__ == "__main__":
    main()
