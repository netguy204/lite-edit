#!/usr/bin/env python3
"""
Prototype: Generate identicons for workspace names at the target tile size.

Tests H1 (are identicons distinguishable at ~48px?) and H2 (do similar
workspace names produce visually distinct identicons?).

Generates a comparison sheet as a PNG file.

Algorithm:
- Hash the workspace name (SHA-256)
- Use hash bytes to determine:
  - A foreground color (hue from hash, saturation/lightness fixed for vibrancy)
  - A 5x5 grid pattern (vertically symmetric, so only 3 columns needed = 15 bits)
- Render at 48x48 pixels per identicon (matching the tile size)
"""

import hashlib
import colorsys
import struct
from pathlib import Path

# Try PIL first, fall back to pure-text output
try:
    from PIL import Image, ImageDraw, ImageFont
    HAS_PIL = True
except ImportError:
    HAS_PIL = False


def generate_identicon_data(name: str):
    """Generate identicon parameters from a workspace name."""
    h = hashlib.sha256(name.encode('utf-8')).digest()

    # Color: use first 2 bytes for hue (0-360), fix saturation and lightness
    hue = (h[0] | (h[1] << 8)) % 360
    # Use byte 2 to slightly vary saturation (0.5-0.8)
    sat = 0.5 + (h[2] / 255.0) * 0.3
    # Use byte 3 to slightly vary lightness (0.4-0.65)
    light = 0.4 + (h[3] / 255.0) * 0.25

    r, g, b = colorsys.hls_to_rgb(hue / 360.0, light, sat)
    fg_color = (int(r * 255), int(g * 255), int(b * 255))

    # Grid: 5x5 with vertical symmetry
    # We need 15 bits (5 rows × 3 columns, mirrored to make 5 cols)
    # Use bytes 4-5 for the pattern bits
    bits = h[4] | (h[5] << 8)

    grid = [[False] * 5 for _ in range(5)]
    for row in range(5):
        for col in range(3):  # Only left half + center
            bit_index = row * 3 + col
            on = bool(bits & (1 << bit_index))
            grid[row][col] = on
            grid[row][4 - col] = on  # Mirror

    return fg_color, grid


def render_identicon_pil(draw, x_offset, y_offset, size, name, label=True):
    """Render a single identicon using PIL at the given offset."""
    fg_color, grid = generate_identicon_data(name)

    # Background for the tile
    bg_color = (30, 30, 36)  # Match TILE_BACKGROUND_COLOR
    draw.rectangle([x_offset, y_offset, x_offset + size - 1, y_offset + size - 1],
                   fill=bg_color)

    # Cell size within the identicon area
    # Reserve some padding and space for potential label
    icon_area = size - 8  # 4px padding each side
    cell_size = icon_area // 5

    # Center the grid
    grid_size = cell_size * 5
    gx = x_offset + (size - grid_size) // 2
    gy = y_offset + (size - grid_size) // 2

    for row in range(5):
        for col in range(5):
            if grid[row][col]:
                cx = gx + col * cell_size
                cy = gy + row * cell_size
                draw.rectangle([cx, cy, cx + cell_size - 1, cy + cell_size - 1],
                               fill=fg_color)

    # Label below
    if label:
        try:
            font = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 10)
        except:
            font = ImageFont.load_default()
        # Truncate long names for display
        display_name = name if len(name) <= 16 else name[:14] + ".."
        bbox = draw.textbbox((0, 0), display_name, font=font)
        tw = bbox[2] - bbox[0]
        tx = x_offset + (size - tw) // 2
        ty = y_offset + size + 2
        draw.text((tx, ty), display_name, fill=(180, 180, 190), font=font)


def render_text_identicon(name: str) -> str:
    """Render an identicon as ASCII art for terminal viewing."""
    fg_color, grid = generate_identicon_data(name)
    lines = []
    lines.append(f"  {name} (color: rgb{fg_color})")
    for row in grid:
        line = "  "
        for cell in row:
            line += "██" if cell else "  "
        lines.append(line)
    lines.append("")
    return "\n".join(lines)


def main():
    # Test workspace names: mix of similar and different names
    workspace_names = [
        # Similar names (testing H2 - hash entropy)
        "project-alpha",
        "project-beta",
        "project-gamma",
        "project-delta",
        # Common naming patterns
        "main",
        "feature/auth",
        "feature/ui",
        "bugfix/crash",
        # Default-like names
        "untitled",
        "untitled-2",
        "workspace-1",
        "workspace-2",
    ]

    if HAS_PIL:
        # Render comparison sheet
        tile_size = 48
        cols = 4
        rows = (len(workspace_names) + cols - 1) // cols
        padding = 16
        label_height = 16

        sheet_w = cols * (tile_size + padding) + padding
        sheet_h = rows * (tile_size + label_height + padding) + padding

        img = Image.new('RGB', (sheet_w, sheet_h), color=(20, 20, 24))
        draw = ImageDraw.Draw(img)

        for i, name in enumerate(workspace_names):
            col = i % cols
            row = i // cols
            x = padding + col * (tile_size + padding)
            y = padding + row * (tile_size + label_height + padding)
            render_identicon_pil(draw, x, y, tile_size, name)

        out_path = Path(__file__).parent / "identicon_comparison.png"
        img.save(str(out_path))
        print(f"Saved comparison sheet to {out_path}")

        # Also render at 2x for easier inspection
        tile_size_2x = 96
        sheet_w_2x = cols * (tile_size_2x + padding) + padding
        sheet_h_2x = rows * (tile_size_2x + label_height + padding) + padding
        img2 = Image.new('RGB', (sheet_w_2x, sheet_h_2x), color=(20, 20, 24))
        draw2 = ImageDraw.Draw(img2)

        for i, name in enumerate(workspace_names):
            col = i % cols
            row = i // cols
            x = padding + col * (tile_size_2x + padding)
            y = padding + row * (tile_size_2x + label_height + padding)
            render_identicon_pil(draw2, x, y, tile_size_2x, name)

        out_path_2x = Path(__file__).parent / "identicon_comparison_2x.png"
        img2.save(str(out_path_2x))
        print(f"Saved 2x comparison sheet to {out_path_2x}")

        # Render the "colored initial" alternative (H3)
        tile_size_alt = 48
        sheet_w_alt = cols * (tile_size_alt + padding) + padding
        sheet_h_alt = rows * (tile_size_alt + label_height + padding) + padding
        img_alt = Image.new('RGB', (sheet_w_alt, sheet_h_alt), color=(20, 20, 24))
        draw_alt = ImageDraw.Draw(img_alt)

        for i, name in enumerate(workspace_names):
            col = i % cols
            row = i // cols
            x = padding + col * (tile_size_alt + padding)
            y = padding + row * (tile_size_alt + label_height + padding)

            fg_color, _ = generate_identicon_data(name)
            # Dimmer version for background
            bg = tuple(c // 3 for c in fg_color)
            draw_alt.rectangle([x, y, x + tile_size_alt - 1, y + tile_size_alt - 1], fill=bg)

            # Draw initial letter centered
            initial = name[0].upper()
            try:
                font = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 24)
            except:
                font = ImageFont.load_default()
            bbox = draw_alt.textbbox((0, 0), initial, font=font)
            tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
            tx = x + (tile_size_alt - tw) // 2
            ty = y + (tile_size_alt - th) // 2 - 2
            draw_alt.text((tx, ty), initial, fill=fg_color, font=font)

            # Label
            try:
                small_font = ImageFont.truetype("/System/Library/Fonts/Menlo.ttc", 10)
            except:
                small_font = ImageFont.load_default()
            display_name = name if len(name) <= 16 else name[:14] + ".."
            bbox2 = draw_alt.textbbox((0, 0), display_name, font=small_font)
            tw2 = bbox2[2] - bbox2[0]
            tx2 = x + (tile_size_alt - tw2) // 2
            ty2 = y + tile_size_alt + 2
            draw_alt.text((tx2, ty2), display_name, fill=(180, 180, 190), font=small_font)

        out_path_alt = Path(__file__).parent / "colored_initial_comparison.png"
        img_alt.save(str(out_path_alt))
        print(f"Saved colored-initial comparison to {out_path_alt}")

    else:
        print("PIL not available, rendering as text:\n")
        for name in workspace_names:
            print(render_text_identicon(name))


if __name__ == "__main__":
    main()
