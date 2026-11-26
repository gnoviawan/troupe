"""Writes sample.figcraft: one page, one frame, text, shapes and a vector.

Usage: python3 -I apps/rune/fixtures/make_sample.py apps/rune/fixtures/sample.figcraft
"""
import json, sys, zipfile

def rgb(h, a=1.0):
    return {"r": int(h[0:2], 16) / 255, "g": int(h[2:4], 16) / 255, "b": int(h[4:6], 16) / 255, "a": a}

def solid(h, opacity=1.0):
    return [{"type": "solid", "color": rgb(h), "opacity": opacity, "visible": True, "blend": "normal"}]

def at(x, y):
    return [1.0, 0.0, 0.0, 1.0, float(x), float(y)]

def text(id, chars, style, size, x, y, w, h, color):
    return {"id": id, "name": chars, "type": "text", "characters": chars,
            "style": {"font_family": "Inter", "font_style": style, "font_size": float(size),
                      "line_height": {"unit": "auto"}, "letter_spacing": {"unit": "percent", "value": 0.0},
                      "decoration": "none", "case": "original", "fills": None, "link": None},
            "parent": 2, "blend": "normal", "transform": at(x, y), "width": float(w), "height": float(h),
            "fills": solid(color)}

def rect(id, name, x, y, w, h, color, radius):
    return {"id": id, "name": name, "type": "rectangle", "parent": 2, "blend": "normal", "transform": at(x, y),
            "width": float(w), "height": float(h), "fills": solid(color),
            "corners": {"radii": [float(radius)] * 4, "smoothing": 0.0}}

mark = [(3, 0), (19, 0), (16, 5), (13, 5), (8, 18), (4, 18), (9, 5), (0, 5)]
nodes = {
    1: {"id": 1, "name": "Page 1", "type": "page", "background": rgb("1E1E1E"), "flows": [],
        "prototype": {"device": "None", "background": rgb("1E1E1E"), "rotation": "portrait"},
        "guides": [], "children": [2], "width": 0.0, "height": 0.0},
    2: {"id": 2, "name": "Sample", "type": "frame", "parent": 1, "children": [3, 4, 5, 6, 7, 8],
        "transform": at(0, 0), "width": 640.0, "height": 400.0, "fills": solid("08090A"),
        "corners": {"radii": [16.0] * 4, "smoothing": 0.0}, "clip": True},
    3: text(3, "Troupe on Rune", "SemiBold", 32, 40, 40, 300, 39, "F7F8F8"),
    4: text(4, "R0 fixture: one frame, text, shapes and a vector", "Regular", 16, 40, 92, 420, 20, "8A8F98"),
    5: rect(5, "Accent", 40, 160, 160, 100, "E4F222", 12),
    6: rect(6, "Info", 220, 160, 160, 100, "5E6AD2", 12),
    7: {"id": 7, "name": "Dot", "type": "ellipse", "start": 0.0, "sweep": 6.283185307179586, "inner": 0.0,
        "parent": 2, "blend": "normal", "transform": at(410, 160), "width": 100.0, "height": 100.0,
        "fills": solid("27A644")},
    8: {"id": 8, "name": "Mark", "type": "vector",
        "network": {"vertices": [{"x": float(x), "y": float(y), "radius": 0.0} for x, y in mark],
                    "segments": [{"a": i, "b": (i + 1) % len(mark), "ta": [0.0, 0.0], "tb": [0.0, 0.0]} for i in range(len(mark))],
                    "regions": [{"loops": [list(range(len(mark)))], "winding": "nonZero"}]},
        "parent": 2, "blend": "normal", "transform": [3.0, 0.0, 0.0, 3.0, 540.0, 300.0], "width": 19.0, "height": 18.0,
        "fills": solid("F7F8F8")},
}
doc = {"format": "figcraft", "version": 1, "imageIndex": [], "name": "Sample", "pages": [1],
       "nodes": {str(k): v for k, v in nodes.items()}, "next_id": 9, "styles": [],
       "variables": {"collections": [], "variables": []}, "current_page": 1, "document_colors": [], "comments": []}
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr(zipfile.ZipInfo("document.json", (2026, 1, 1, 0, 0, 0)), json.dumps(doc, indent=1), zipfile.ZIP_DEFLATED)
