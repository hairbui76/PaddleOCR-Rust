#!/usr/bin/env python3
"""Generate the `PDF-001` entry-gate fidelity corpus.

Roadmap item: `PDF-001`, entry gate part 2
(`docs/ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md` section 5).

The gate requires a fidelity measurement over "a small committed corpus that
includes a scanned page, a vector page, an embedded CID font, and a page with a
form XObject". Every byte here is authored by this script, so the corpus carries
this project's own licence and no third-party font, image, or document is
redistributed. That is the whole reason it is generated rather than collected:
a real-world PDF corpus would drag in licensing this project cannot clear.

Five documents:

    vector.pdf        paths, fills, strokes, a bezier, a rotated matrix
    scanned_flate.pdf a full-page RGB image XObject, FlateDecode
    scanned_jpeg.pdf  the same raster through DCTDecode, the real scan path
    form_xobject.pdf  one Form XObject with its own BBox and Matrix, drawn twice
    cid_font.pdf      Type0/Identity-H over an embedded CIDFontType2

The CID font is also generated: `build_truetype` emits a minimal but valid
TrueType with four glyphs of plain straight-line contours. A hand-written font
is the only way to have an embedded CID font whose licence is unambiguous.

`scanned_jpeg.pdf` needs `cv2` for the JPEG encode, which is the same encoder
this project's other captures already went through. Everything else is pure
standard library.

Usage:
    python3 tools/generate_pdf_fidelity_corpus.py <output-directory>
"""

from __future__ import annotations

import struct
import sys
import zlib
from pathlib import Path

# ---------------------------------------------------------------------------
# A minimal PDF writer
# ---------------------------------------------------------------------------


class Pdf:
    """Builds a one-page PDF, tracking object offsets for the xref table."""

    def __init__(self):
        self.objects: list[bytes | None] = []

    def reserve(self) -> int:
        self.objects.append(None)
        return len(self.objects)

    def put(self, number: int, body: bytes) -> int:
        self.objects[number - 1] = body
        return number

    def add(self, body: bytes) -> int:
        return self.put(self.reserve(), body)

    def stream(self, dictionary: str, data: bytes) -> int:
        head = f"<< {dictionary} /Length {len(data)} >>\nstream\n".encode("latin-1")
        return self.add(head + data + b"\nendstream")

    def serialize(self, root: int) -> bytes:
        out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offsets = [0] * (len(self.objects) + 1)
        for index, body in enumerate(self.objects, start=1):
            if body is None:
                raise ValueError(f"object {index} was reserved and never written")
            offsets[index] = len(out)
            out += f"{index} 0 obj\n".encode("latin-1")
            out += body
            out += b"\nendobj\n"
        start_xref = len(out)
        count = len(self.objects) + 1
        out += f"xref\n0 {count}\n".encode("latin-1")
        out += b"0000000000 65535 f \n"
        for index in range(1, count):
            out += f"{offsets[index]:010d} 00000 n \n".encode("latin-1")
        out += f"trailer\n<< /Size {count} /Root {root} 0 R >>\n".encode("latin-1")
        out += f"startxref\n{start_xref}\n%%EOF\n".encode("latin-1")
        return bytes(out)


def one_page(pdf: Pdf, width: float, height: float, content: bytes, resources: str) -> int:
    """Writes the catalogue, page tree, and one page. Returns the catalogue."""
    contents = pdf.stream("", content)
    pages = pdf.reserve()
    page = pdf.add(
        f"<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {width} {height}] "
        f"/Resources << {resources} >> /Contents {contents} 0 R >>".encode("latin-1")
    )
    pdf.put(
        pages,
        f"<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>".encode("latin-1"),
    )
    return pdf.add(f"<< /Type /Catalog /Pages {pages} 0 R >>".encode("latin-1"))


# ---------------------------------------------------------------------------
# A minimal TrueType font
# ---------------------------------------------------------------------------


def _glyph(contours: list[list[tuple[int, int]]]) -> bytes:
    """A simple glyph: straight-line contours, all points on-curve."""
    if not contours:
        return b""
    xs = [x for contour in contours for x, _ in contour]
    ys = [y for contour in contours for _, y in contour]
    out = struct.pack(">hhhhh", len(contours), min(xs), min(ys), max(xs), max(ys))
    end_points = []
    total = 0
    for contour in contours:
        total += len(contour)
        end_points.append(total - 1)
    out += b"".join(struct.pack(">H", value) for value in end_points)
    out += struct.pack(">H", 0)  # no instructions
    points = [point for contour in contours for point in contour]
    out += bytes([0x01] * len(points))  # every point on-curve, x and y as int16
    previous = 0
    for x, _ in points:
        out += struct.pack(">h", x - previous)
        previous = x
    previous = 0
    for _, y in points:
        out += struct.pack(">h", y - previous)
        previous = y
    if len(out) % 4:
        out += b"\x00" * (4 - len(out) % 4)
    return out


def build_truetype() -> bytes:
    """A four-glyph TrueType: .notdef, a square, a triangle, and an H.

    Straight lines only, one instruction-free glyph each, and every table a
    PDF-embedded CIDFontType2 is read through. Authored here so the corpus
    embeds a font whose licence is this project's own.
    """
    units_per_em = 1000
    ascent, descent = 800, -200

    glyphs = [
        _glyph([]),  # .notdef, empty
        _glyph([[(100, 0), (700, 0), (700, 700), (100, 700)]]),
        _glyph([[(100, 0), (700, 0), (400, 700)]]),
        _glyph(
            [
                [(100, 0), (250, 0), (250, 300), (550, 300), (550, 0), (700, 0),
                 (700, 700), (550, 700), (550, 450), (250, 450), (250, 700), (100, 700)]
            ]
        ),
    ]
    advances = [500, 800, 800, 800]

    glyf = b"".join(glyphs)
    offsets = [0]
    for glyph in glyphs:
        offsets.append(offsets[-1] + len(glyph))
    # Long-format loca, which is what indexToLocFormat 1 declares.
    loca = b"".join(struct.pack(">I", offset) for offset in offsets)

    head = struct.pack(
        ">IIIIHHqqhhhhHHhhh",
        0x00010000,  # version
        0x00010000,  # fontRevision
        0,  # checkSumAdjustment, left zero
        0x5F0F3CF5,  # magicNumber
        0b1011,  # flags
        units_per_em,
        0,  # created
        0,  # modified
        0,
        descent,
        units_per_em,
        ascent,
        0,  # macStyle
        8,  # lowestRecPPEM
        2,  # fontDirectionHint
        1,  # indexToLocFormat: long
        0,  # glyphDataFormat
    )
    hhea = struct.pack(
        ">IhhhHhhhhhhhhhhhH",
        0x00010000,
        ascent,
        descent,
        0,  # lineGap
        max(advances),
        0,  # minLeftSideBearing
        0,  # minRightSideBearing
        max(advances),
        1,  # caretSlopeRise
        0,
        0,
        0,
        0,
        0,
        0,
        0,  # metricDataFormat
        len(glyphs),  # numberOfHMetrics
    )
    maxp = struct.pack(
        ">IHHHHHHHHHHHHHH",
        0x00010000,
        len(glyphs),
        12,  # maxPoints: the H, the largest contour authored here
        1,  # maxContours
        0,  # maxCompositePoints
        0,  # maxCompositeContours
        2,  # maxZones
        0,  # maxTwilightPoints
        0,  # maxStorage
        0,  # maxFunctionDefs
        0,  # maxInstructionDefs
        0,  # maxStackElements
        0,  # maxSizeOfInstructions
        0,  # maxComponentElements
        0,  # maxComponentDepth
    )
    hmtx = b"".join(struct.pack(">Hh", advance, 100) for advance in advances)
    # cmap format 4, mapping 'A'..'C' onto glyphs 1..3. Identity-H does not read
    # it, but a renderer that validates the font does.
    segments = [(0x41, 0x43, 1), (0xFFFF, 0xFFFF, 0)]
    seg_count = len(segments)
    end_codes = b"".join(struct.pack(">H", end) for _, end, _ in segments)
    start_codes = b"".join(struct.pack(">H", start) for start, _, _ in segments)
    # idDelta is an int16 whose arithmetic is modulo 65536, so it is packed
    # unsigned and masked rather than range-checked as a signed value.
    id_deltas = b"".join(
        struct.pack(">H", (glyph - start) & 0xFFFF if glyph else 1)
        for start, _, glyph in segments
    )
    id_range_offsets = b"".join(struct.pack(">H", 0) for _ in segments)
    subtable = (
        struct.pack(
            ">HHHHHHH",
            4,
            16 + 8 * seg_count,
            0,
            seg_count * 2,
            2,
            0,
            0,
        )
        + end_codes
        + struct.pack(">H", 0)
        + start_codes
        + id_deltas
        + id_range_offsets
    )
    cmap = struct.pack(">HHHHI", 0, 1, 3, 1, 12) + subtable
    name = struct.pack(">HHH", 0, 0, 6)
    post = struct.pack(">IIhhIIIII", 0x00030000, 0, 0, 0, 0, 0, 0, 0, 0)
    os2 = struct.pack(">H", 4) + b"\x00" * 84

    tables = {
        b"OS/2": os2,
        b"cmap": cmap,
        b"glyf": glyf,
        b"head": head,
        b"hhea": hhea,
        b"hmtx": hmtx,
        b"loca": loca,
        b"maxp": maxp,
        b"name": name,
        b"post": post,
    }

    def checksum(data: bytes) -> int:
        padded = data + b"\x00" * (-len(data) % 4)
        total = 0
        for index in range(0, len(padded), 4):
            total = (total + struct.unpack(">I", padded[index : index + 4])[0]) & 0xFFFFFFFF
        return total

    count = len(tables)
    entry_selector = max(0, count.bit_length() - 1)
    search_range = (1 << entry_selector) * 16
    out = struct.pack(
        ">IHHHH", 0x00010000, count, search_range, entry_selector, count * 16 - search_range
    )
    offset = 12 + 16 * count
    directory = b""
    body = b""
    for tag in sorted(tables):
        data = tables[tag]
        directory += tag + struct.pack(">III", checksum(data), offset, len(data))
        padded = data + b"\x00" * (-len(data) % 4)
        body += padded
        offset += len(padded)
    return out + directory + body


# ---------------------------------------------------------------------------
# The five documents
# ---------------------------------------------------------------------------


def vector_pdf() -> bytes:
    pdf = Pdf()
    content = b"""q
0.15 0.15 0.15 rg
20 20 160 40 re f
Q
q
0.85 0.2 0.2 RG
4 w
20 80 m 180 80 l S
Q
q
0.2 0.3 0.8 rg
20 100 m 60 180 100 180 140 100 c f
Q
q
1 0 0 1 100 20 cm
0.5 0.7 0.2 rg
0 0 40 40 re f
Q
"""
    root = one_page(pdf, 200, 200, content, "")
    return pdf.serialize(root)


def raster(width: int, height: int) -> bytes:
    """A deterministic RGB raster with hard edges and a gradient."""
    rows = bytearray()
    for y in range(height):
        for x in range(width):
            if (x // 8 + y // 8) % 2 == 0:
                rows += bytes((20, 20, 20))
            elif x < width // 3:
                rows += bytes((240, 240, 240))
            else:
                rows += bytes((min(255, x * 3 % 256), 128, max(0, 255 - y * 3) % 256))
    return bytes(rows)


def scanned_flate_pdf(width: int, height: int) -> bytes:
    pdf = Pdf()
    data = zlib.compress(raster(width, height), 9)
    image = pdf.stream(
        f"/Type /XObject /Subtype /Image /Width {width} /Height {height} "
        f"/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode",
        data,
    )
    content = f"q {width * 2} 0 0 {height * 2} 0 0 cm /Im0 Do Q\n".encode("latin-1")
    root = one_page(
        pdf, width * 2, height * 2, content, f"/XObject << /Im0 {image} 0 R >>"
    )
    return pdf.serialize(root)


def scanned_jpeg_pdf(width: int, height: int) -> bytes | None:
    try:
        import cv2
        import numpy as np
    except ImportError:
        return None
    flat = np.frombuffer(raster(width, height), dtype=np.uint8)
    rgb = flat.reshape(height, width, 3)
    bgr = rgb[:, :, ::-1].copy()
    ok, encoded = cv2.imencode(".jpg", bgr, [int(cv2.IMWRITE_JPEG_QUALITY), 92])
    if not ok:
        return None
    pdf = Pdf()
    image = pdf.stream(
        f"/Type /XObject /Subtype /Image /Width {width} /Height {height} "
        f"/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode",
        encoded.tobytes(),
    )
    content = f"q {width * 2} 0 0 {height * 2} 0 0 cm /Im0 Do Q\n".encode("latin-1")
    root = one_page(
        pdf, width * 2, height * 2, content, f"/XObject << /Im0 {image} 0 R >>"
    )
    return pdf.serialize(root)


def form_xobject_pdf() -> bytes:
    pdf = Pdf()
    form_content = b"""0.2 0.4 0.7 rg
0 0 50 30 re f
0 0 0 RG
2 w
0 0 m 50 30 l S
"""
    form = pdf.stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 50 30] /Matrix [1 0 0 1 0 0]",
        form_content,
    )
    # Drawn twice, the second time scaled and translated, so a renderer that
    # ignores either the form matrix or the invoking matrix differs visibly.
    content = b"""q 1 0 0 1 20 20 cm /Fm0 Do Q
q 1.5 0 0 1.5 20 90 cm /Fm0 Do Q
"""
    root = one_page(pdf, 200, 200, content, f"/XObject << /Fm0 {form} 0 R >>")
    return pdf.serialize(root)


def cid_font_pdf(font: bytes) -> bytes:
    pdf = Pdf()
    font_file = pdf.stream(
        f"/Length1 {len(font)}",
        font,
    )
    descriptor = pdf.add(
        f"<< /Type /FontDescriptor /FontName /GateTest /Flags 4 "
        f"/FontBBox [0 -200 800 800] /ItalicAngle 0 /Ascent 800 /Descent -200 "
        f"/CapHeight 700 /StemV 80 /FontFile2 {font_file} 0 R >>".encode("latin-1")
    )
    descendant = pdf.add(
        f"<< /Type /Font /Subtype /CIDFontType2 /BaseFont /GateTest "
        f"/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> "
        f"/FontDescriptor {descriptor} 0 R /DW 800 /CIDToGIDMap /Identity >>".encode(
            "latin-1"
        )
    )
    font_object = pdf.add(
        f"<< /Type /Font /Subtype /Type0 /BaseFont /GateTest "
        f"/Encoding /Identity-H /DescendantFonts [{descendant} 0 R] >>".encode("latin-1")
    )
    # Identity-H: two bytes per glyph index. Glyphs 1, 2, 3 are the square, the
    # triangle, and the H.
    content = b"""BT
/F0 48 Tf
20 120 Td
<000100020003> Tj
0 -60 Td
<000300020001> Tj
ET
"""
    root = one_page(pdf, 200, 200, content, f"/Font << /F0 {font_object} 0 R >>")
    return pdf.serialize(root)


def shading_pdf() -> bytes:
    """An axial shading, the feature the ADR names as silently droppable.

    `sh` inside a clip, so a renderer that ignores the shading leaves white
    where the reference leaves a gradient - a difference no antialiasing
    tolerance can explain away.
    """
    pdf = Pdf()
    shading = pdf.add(
        b"<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [20 0 180 0] "
        b"/Function << /FunctionType 2 /Domain [0 1] /C0 [0.05 0.1 0.6] "
        b"/C1 [0.95 0.8 0.1] /N 1 >> /Extend [true true] >>"
    )
    content = b"""q
20 20 160 70 re W n
/Sh0 sh
Q
q
0 0 0 rg
20 110 160 10 re f
Q
"""
    root = one_page(pdf, 200, 200, content, f"/Shading << /Sh0 {shading} 0 R >>")
    return pdf.serialize(root)


def standard_font_pdf() -> bytes:
    """Text in Helvetica, which is **not** embedded.

    One of the fourteen standard fonts, so each renderer supplies its own
    substitute rather than reading a font program from the file. That makes this
    the corpus's only page where the two renderers are not being asked to
    reproduce the same glyph outlines - and the only page whose text an OCR pass
    can be run over, which is what turns a pixel difference into a consequence.
    """
    pdf = Pdf()
    font = pdf.add(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
        b"/Encoding /WinAnsiEncoding >>"
    )
    content = b"""BT
/F0 36 Tf
30 300 Td
(Hello World) Tj
0 -60 Td
(Rust OCR 2026) Tj
0 -60 Td
(PADDLE ocr test) Tj
ET
"""
    root = one_page(pdf, 400, 400, content, f"/Font << /F0 {font} 0 R >>")
    return pdf.serialize(root)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    directory = Path(sys.argv[1])
    directory.mkdir(parents=True, exist_ok=True)

    font = build_truetype()
    (directory / "gate-test-font.ttf").write_bytes(font)

    documents = {
        "vector.pdf": vector_pdf(),
        "scanned_flate.pdf": scanned_flate_pdf(60, 80),
        "form_xobject.pdf": form_xobject_pdf(),
        "cid_font.pdf": cid_font_pdf(font),
        "shading.pdf": shading_pdf(),
        "standard_font.pdf": standard_font_pdf(),
    }
    jpeg = scanned_jpeg_pdf(60, 80)
    if jpeg is None:
        print("cv2 unavailable: scanned_jpeg.pdf not written", file=sys.stderr)
    else:
        documents["scanned_jpeg.pdf"] = jpeg

    for name, data in sorted(documents.items()):
        (directory / name).write_bytes(data)
        print(f"{name}: {len(data)} bytes")
    print(f"gate-test-font.ttf: {len(font)} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
