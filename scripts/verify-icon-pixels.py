"""Render the actual EXE icon resource with DrawIconEx; never launch the app."""
import ctypes as c
from ctypes import wintypes as w
import json
from pathlib import Path
import sys
from PIL import Image, ImageDraw

exe, source, output = (Path(value).resolve() for value in sys.argv[1:])
kernel = c.WinDLL("kernel32", use_last_error=True)
user = c.WinDLL("user32", use_last_error=True)
gdi = c.WinDLL("gdi32", use_last_error=True)
kernel.LoadLibraryExW.argtypes = [w.LPCWSTR, w.HANDLE, w.DWORD]
kernel.LoadLibraryExW.restype = w.HMODULE
kernel.FreeLibrary.argtypes = [w.HMODULE]
user.LoadImageW.argtypes = [w.HINSTANCE, c.c_void_p, w.UINT, c.c_int, c.c_int, w.UINT]
user.LoadImageW.restype = w.HANDLE
user.DestroyIcon.argtypes = [w.HICON]
user.DrawIconEx.argtypes = [w.HDC, c.c_int, c.c_int, w.HICON, c.c_int, c.c_int, w.UINT, w.HBRUSH, w.UINT]
gdi.CreateCompatibleDC.argtypes = [w.HDC]
gdi.CreateCompatibleDC.restype = w.HDC
gdi.CreateDIBSection.argtypes = [w.HDC, c.c_void_p, w.UINT, c.POINTER(c.c_void_p), w.HANDLE, w.DWORD]
gdi.CreateDIBSection.restype = w.HBITMAP
gdi.SelectObject.argtypes = [w.HDC, w.HGDIOBJ]
gdi.SelectObject.restype = w.HGDIOBJ
gdi.DeleteObject.argtypes = [w.HGDIOBJ]
gdi.DeleteDC.argtypes = [w.HDC]

module = kernel.LoadLibraryExW(str(exe), None, 0x22)  # data/resource only; no DllMain/EXE execution
if not module:
    raise c.WinError(c.get_last_error())
icon_file = Image.open(source)
sizes = sorted(size[0] for size in icon_file.ico.sizes())
sheet = Image.new("RGB", (sum(max(64, n + 16) for n in sizes), 600), "#e7e7e7")
draw = ImageDraw.Draw(sheet)
rows = []
left = 0
try:
    for size in sizes:
        source_frame = icon_file.ico.getimage((size, size)).convert("RGBA")
        bounds = source_frame.getchannel("A").getbbox()
        assert bounds is not None, f"Fully transparent {size}px icon"
        visible_bounds = source_frame.getchannel("A").point(lambda alpha: 255 if alpha >= 128 else 0).getbbox()
        assert visible_bounds is not None, f"No visible artwork in {size}px icon"
        rows.append({"size": size, "alpha_bounds": bounds, "visible_bounds": visible_bounds})
        draw.text((left + 3, 4), str(size), fill="black")
        icon = user.LoadImageW(module, c.c_void_p(1), 1, size, size, 0)
        if not icon:
            raise c.WinError(c.get_last_error())
        try:
            for row, background in enumerate((255, 28)):
                dc = gdi.CreateCompatibleDC(None)
                header = (w.DWORD * 10)(40, size, -size & 0xFFFFFFFF, 1 | (32 << 16), 0, size * size * 4, 0, 0, 0, 0)
                bits = c.c_void_p()
                bitmap = gdi.CreateDIBSection(dc, c.byref(header), 0, c.byref(bits), None, 0)
                assert dc and bitmap and bits.value, "Cannot allocate native icon bitmap"
                previous = gdi.SelectObject(dc, bitmap)
                try:
                    c.memset(bits, background, size * size * 4)
                    assert user.DrawIconEx(dc, 0, 0, icon, size, size, 0, None, 3), "DrawIconEx failed"
                    pixels = c.string_at(bits, size * size * 4)
                    frame = Image.frombytes("RGB", (size, size), pixels, "raw", "BGRX")
                    assert frame.getextrema() != ((background, background),) * 3, "Blank native resource render"
                    sheet.paste(frame, (left + 4, 24 + row * 280))
                finally:
                    gdi.SelectObject(dc, previous)
                    gdi.DeleteObject(bitmap)
                    gdi.DeleteDC(dc)
        finally:
            user.DestroyIcon(icon)
        left += max(64, size + 16)
finally:
    kernel.FreeLibrary(module)
output.parent.mkdir(parents=True, exist_ok=True)
sheet.save(output)
report = {"executable": str(exe), "source": str(source), "proof": "LoadLibraryEx resource-only -> LoadImageW group 1 -> DrawIconEx memory DIB", "surfaces": "light/dark native-size icon resource proof; no application/desktop windows, no OS tray interaction", "frames": rows}
output.with_suffix(".json").write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps(report, indent=2))
