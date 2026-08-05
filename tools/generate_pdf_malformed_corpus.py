#!/usr/bin/env python3
"""Generate the `PDF-001` entry-gate malformed corpus.

Roadmap item: `PDF-001`, entry gate part 4
(`docs/ADR_DOCIO_DEC_001_PDF_AND_OFFICE.md` section 5).

The gate requires "a malformed-input corpus exercised without panic, unbounded
allocation, or execution of anything embedded in the document". A random fuzz
campaign shows that nothing crashed on the inputs it happened to try; a **named**
corpus shows that each specific way a PDF can lie is answered. `SEC-IMG-001`
drew that distinction for images and this follows it.

Each document is one named lie, built by damaging a valid document so that the
damage is the only variable:

    truncated_header        the first bytes are not a PDF at all
    truncated_body          valid header, file cut inside an object
    no_xref                 the cross-reference table is gone
    bad_xref_offsets        the xref points at bytes that are not objects
    missing_root            the trailer names a catalogue that does not exist
    circular_pages          the page tree is its own parent
    huge_declared_length    a stream declaring far more bytes than the file has
    huge_image_dimensions   an image XObject declaring 60000x60000 pixels
    negative_dimensions     a MediaBox with a negative extent
    deep_nesting            a form XObject drawing itself, 200 levels declared
    javascript_openaction   an /OpenAction that runs JavaScript
    embedded_file           an /EmbeddedFile, which must never be written out
    encrypted_stub          an /Encrypt dictionary with no supplied password
    zero_pages              a valid catalogue whose page tree is empty

The last three are the ones that matter beyond crashing: a renderer that runs
the JavaScript, extracts the embedded file, or silently renders an encrypted
document has done something worse than fail.

Pure standard library. Nothing is downloaded.

Usage:
    python3 tools/generate_pdf_malformed_corpus.py <output-directory>
"""

from __future__ import annotations

import sys
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from generate_pdf_fidelity_corpus import Pdf, one_page, vector_pdf


def truncated_header() -> bytes:
    return b"%PDF" + vector_pdf()[200:400]


def truncated_body() -> bytes:
    valid = vector_pdf()
    return valid[: len(valid) // 2]


def no_xref() -> bytes:
    valid = vector_pdf()
    index = valid.rfind(b"xref")
    return valid[:index] + b"%%EOF\n"


def bad_xref_offsets() -> bytes:
    valid = vector_pdf()
    # Every ten-digit offset becomes an offset past the end of the file.
    out = bytearray(valid)
    start = out.rfind(b"xref")
    end = out.rfind(b"trailer")
    segment = bytes(out[start:end])
    damaged = bytearray()
    for line in segment.split(b"\n"):
        if len(line) >= 10 and line[:10].isdigit() and line != b"0000000000 65535 f ":
            damaged += b"9999999999" + line[10:] + b"\n"
        else:
            damaged += line + b"\n"
    return bytes(out[:start]) + bytes(damaged[:-1]) + bytes(out[end:])


def missing_root() -> bytes:
    valid = vector_pdf()
    return valid.replace(b"/Root ", b"/Root 999 0 R % ", 1)


def circular_pages() -> bytes:
    pdf = Pdf()
    contents = pdf.stream("", b"0 0 0 rg 10 10 50 50 re f\n")
    pages = pdf.reserve()
    page = pdf.add(
        f"<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 100 100] "
        f"/Resources << >> /Contents {contents} 0 R >>".encode("latin-1")
    )
    # The page tree lists itself as a kid alongside the page.
    pdf.put(
        pages,
        f"<< /Type /Pages /Kids [{pages} 0 R {page} 0 R] /Count 2 "
        f"/Parent {pages} 0 R >>".encode("latin-1"),
    )
    root = pdf.add(f"<< /Type /Catalog /Pages {pages} 0 R >>".encode("latin-1"))
    return pdf.serialize(root)


def huge_declared_length() -> bytes:
    valid = vector_pdf()
    return valid.replace(b"/Length ", b"/Length 4000000000 % ", 1)


def huge_image_dimensions() -> bytes:
    pdf = Pdf()
    data = zlib.compress(b"\x00" * 300, 9)
    image = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 60000 /Height 60000 "
        "/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode",
        data,
    )
    content = b"q 100 0 0 100 0 0 cm /Im0 Do Q\n"
    root = one_page(pdf, 100, 100, content, f"/XObject << /Im0 {image} 0 R >>")
    return pdf.serialize(root)


def negative_dimensions() -> bytes:
    pdf = Pdf()
    contents = pdf.stream("", b"0 0 0 rg 10 10 50 50 re f\n")
    pages = pdf.reserve()
    page = pdf.add(
        f"<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 -500 -500] "
        f"/Resources << >> /Contents {contents} 0 R >>".encode("latin-1")
    )
    pdf.put(pages, f"<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>".encode("latin-1"))
    root = pdf.add(f"<< /Type /Catalog /Pages {pages} 0 R >>".encode("latin-1"))
    return pdf.serialize(root)


def deep_nesting() -> bytes:
    pdf = Pdf()
    form = pdf.reserve()
    # The form draws itself: a renderer without a recursion bound never returns.
    body = b"q 0.99 0 0 0.99 1 1 cm /Fm0 Do Q\n0 0 0 rg 0 0 10 10 re f\n"
    head = (
        f"<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] "
        f"/Resources << /XObject << /Fm0 {form} 0 R >> >> /Length {len(body)} >>\n"
        f"stream\n"
    ).encode("latin-1")
    pdf.put(form, head + body + b"\nendstream")
    content = b"q /Fm0 Do Q\n"
    root = one_page(pdf, 100, 100, content, f"/XObject << /Fm0 {form} 0 R >>")
    return pdf.serialize(root)


def javascript_openaction() -> bytes:
    pdf = Pdf()
    contents = pdf.stream("", b"0 0 0 rg 10 10 50 50 re f\n")
    pages = pdf.reserve()
    page = pdf.add(
        f"<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 100 100] "
        f"/Resources << >> /Contents {contents} 0 R >>".encode("latin-1")
    )
    pdf.put(pages, f"<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>".encode("latin-1"))
    # The script writes a file. If it ever runs, the file appears next to the
    # corpus and the harness reports it.
    script = pdf.add(
        b"<< /Type /Action /S /JavaScript /JS "
        b"(try { app.launchURL('file:///tmp/pdf-gate-js-ran'); "
        b"this.exportDataObject({ cName: 'evidence', nLaunch: 2 }); } catch (e) {})"
        b" >>"
    )
    root = pdf.add(
        f"<< /Type /Catalog /Pages {pages} 0 R /OpenAction {script} 0 R "
        f"/Names << /JavaScript << /Names [(gate) {script} 0 R] >> >> >>".encode("latin-1")
    )
    return pdf.serialize(root)


def embedded_file() -> bytes:
    pdf = Pdf()
    contents = pdf.stream("", b"0 0 0 rg 10 10 50 50 re f\n")
    pages = pdf.reserve()
    page = pdf.add(
        f"<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 100 100] "
        f"/Resources << >> /Contents {contents} 0 R >>".encode("latin-1")
    )
    pdf.put(pages, f"<< /Type /Pages /Kids [{page} 0 R] /Count 1 >>".encode("latin-1"))
    payload = b"PDF-GATE-EMBEDDED-PAYLOAD-MUST-NOT-BE-WRITTEN\n"
    stream = pdf.stream("/Type /EmbeddedFile /Subtype /text#2Fplain", payload)
    spec = pdf.add(
        f"<< /Type /Filespec /F (payload.txt) /UF (payload.txt) "
        f"/EF << /F {stream} 0 R >> >>".encode("latin-1")
    )
    root = pdf.add(
        f"<< /Type /Catalog /Pages {pages} 0 R "
        f"/Names << /EmbeddedFiles << /Names [(payload.txt) {spec} 0 R] >> >> >>".encode(
            "latin-1"
        )
    )
    return pdf.serialize(root)


def encrypted_stub() -> bytes:
    """An /Encrypt dictionary the file does not actually honour.

    The point is not to build real encryption: it is that a document declaring
    itself encrypted must not be rendered as though it were not.
    """
    valid = vector_pdf()
    encrypt = (
        b"<< /Filter /Standard /V 2 /R 3 /Length 128 /P -1 "
        b"/O <0102030405060708090a0b0c0d0e0f10> "
        b"/U <1112131415161718191a1b1c1d1e1f20> >>"
    )
    return valid.replace(b"/Size ", b"/Encrypt " + encrypt + b" /Size ", 1)


def zero_pages() -> bytes:
    pdf = Pdf()
    pages = pdf.add(b"<< /Type /Pages /Kids [] /Count 0 >>")
    root = pdf.add(f"<< /Type /Catalog /Pages {pages} 0 R >>".encode("latin-1"))
    return pdf.serialize(root)


CASES = {
    "truncated_header": truncated_header,
    "truncated_body": truncated_body,
    "no_xref": no_xref,
    "bad_xref_offsets": bad_xref_offsets,
    "missing_root": missing_root,
    "circular_pages": circular_pages,
    "huge_declared_length": huge_declared_length,
    "huge_image_dimensions": huge_image_dimensions,
    "negative_dimensions": negative_dimensions,
    "deep_nesting": deep_nesting,
    "javascript_openaction": javascript_openaction,
    "embedded_file": embedded_file,
    "encrypted_stub": encrypted_stub,
    "zero_pages": zero_pages,
}


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    directory = Path(sys.argv[1])
    directory.mkdir(parents=True, exist_ok=True)
    for name, build in sorted(CASES.items()):
        data = build()
        (directory / f"{name}.pdf").write_bytes(data)
        print(f"{name}.pdf: {len(data)} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
