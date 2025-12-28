"""Render the application's original book geometry at Windows icon sizes.

The matching editable vector is assets/icon.svg. Pillow is needed only when
regenerating committed assets, never during a Cargo build or in the application.
"""
from pathlib import Path
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
SCALE = 16
canvas = Image.new("RGBA", (64 * SCALE, 64 * SCALE), (0, 0, 0, 0))
draw = ImageDraw.Draw(canvas)
draw.rounded_rectangle((4*SCALE, 4*SCALE, 60*SCALE-1, 60*SCALE-1), radius=16*SCALE, fill="#107469")
points = [(32,21),(16,16),(16,45),(32,50),(48,45),(48,16),(32,21)]
draw.line([(x*SCALE,y*SCALE) for x,y in points], fill="white", width=3*SCALE, joint="curve")
draw.line([(32*SCALE,21*SCALE),(32*SCALE,50*SCALE)], fill="white", width=3*SCALE)
for x,y in points:
    r=1.5*SCALE
    draw.ellipse((x*SCALE-r,y*SCALE-r,x*SCALE+r,y*SCALE+r),fill="white")
icon = canvas.resize((256,256), Image.Resampling.LANCZOS)
icon.save(ROOT / "assets/icon.png")
sizes = [(n,n) for n in (16,20,24,32,40,48,64,128,256)]
icon.save(ROOT / "assets/text-reader.ico",format="ICO",sizes=sizes)
print("Generated original PNG and multi-resolution Windows ICO:", sizes)
