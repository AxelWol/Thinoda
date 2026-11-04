from __future__ import annotations

import argparse
import copy
import hashlib
import io
import json
import math
import struct
import sys
import zipfile
import zlib
from pathlib import Path


IMAGE_COUNT = 50
IMAGE_WIDTH = 256
IMAGE_HEIGHT = 192
IMAGE_MARKER = (17, 241, 141)


def png_chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data))
    )


def synthetic_image(index: int) -> bytes:
    """Generate a distinct, bounded RGB PNG without external image libraries."""
    palette = [
        bytes(((33 + 7 * i) % 256, (37 + 11 * i) % 256, (53 + 13 * i) % 256))
        for i in range(16)
    ]
    marker = bytes(IMAGE_MARKER)
    state = 0x9E3779B9 ^ (index + 1)
    pixels = bytearray()
    for y in range(IMAGE_HEIGHT):
        pixels.append(0)  # PNG filter: None
        for x in range(IMAGE_WIDTH):
            # A flat quadrant is a paint sentinel; the rest is distinct synthetic texture.
            if x < IMAGE_WIDTH // 2 and y < IMAGE_HEIGHT // 2:
                pixels.extend(marker)
            else:
                state ^= (state << 13) & 0xFFFFFFFF
                state ^= state >> 17
                state ^= (state << 5) & 0xFFFFFFFF
                pixels.extend(palette[state >> 28])
    header = struct.pack(">IIBBBBB", IMAGE_WIDTH, IMAGE_HEIGHT, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", header)
        + png_chunk(b"IDAT", zlib.compress(bytes(pixels), level=6))
        + png_chunk(b"IEND", b"")
    )


def build_archive(source: Path, strokes: int, embedded_images: bool = False) -> tuple[bytes, int]:
    with zipfile.ZipFile(source) as archive:
        source_entries = archive.namelist()
        manifest = archive.read("manifest.json")
        workbook = json.loads(archive.read("content.json"))

    resources = []
    blobs = {}
    if embedded_images:
        manifest_data = json.loads(manifest)
        if (
            manifest_data.get("format") != "thinoda"
            or manifest_data.get("version") != 6
            or manifest_data.get("resources") != []
            or len(workbook["sheets"]) != 1
            or workbook["sheets"][0].get("resources", {}).get("entries", {})
            or sorted(source_entries) != ["content.json", "manifest.json"]
        ):
            raise ValueError("embedded fixtures require a single-sheet v6 source without resources")
        for index in range(IMAGE_COUNT):
            image = synthetic_image(index)
            digest = hashlib.sha256(image).hexdigest()
            if digest in blobs:
                raise ValueError("synthetic images must have distinct resource IDs")
            blobs[digest] = image
            resources.append(
                {
                    "sheet": 0,
                    "id": digest,
                    "name": f"synthetic-{index:02}.png",
                    "media_type": "image/png",
                    "size": len(image),
                }
            )
        manifest_data["resources"] = sorted(resources, key=lambda resource: resource["id"])
        manifest = json.dumps(manifest_data, separators=(",", ":")).encode()

    sheet = workbook["sheets"][0]
    root = copy.deepcopy(sheet["topics"][str(sheet["root"])])
    if embedded_images and (root.get("image") or root.get("attachment") or root.get("children")):
        raise ValueError("embedded fixtures require an empty root without resource references")
    prototype = next(topic for topic in sheet["topics"].values() if topic.get("card"))
    sheet["title"] = f"Performance {strokes}"
    sheet["root_mode"] = "hidden"
    sheet["topics"] = {str(root["id"]): root}
    sheet["floating"] = []
    sheet["zones"] = []
    for index in range(IMAGE_COUNT):
        topic = copy.deepcopy(prototype)
        topic["id"] = index + 2
        topic["title"] = f"Image {index}"
        topic["position"] = {"x": float(index * 160), "y": -180.0}
        topic["card"] = {"kind": "image", "size": {"x": 140.0, "y": 100.0}}
        if embedded_images:
            topic["image"] = f"thinoda-resource:{resources[index]['id']}"
            topic["image_size"] = {"x": 96.0, "y": 72.0}
        sheet["topics"][str(topic["id"])] = topic
        sheet["floating"].append(topic["id"])

    objects = []
    for index in range(strokes):
        x, y = float((index % 100) * 80), float((index // 100) * 80)
        points = [
            {"x": x + offset, "y": round(y + math.sin(offset * 0.2) * 6.0, 4)}
            for offset in range(64)
        ]
        objects.append(
            {
                "id": index + 52,
                "kind": {
                    "ink": {
                        "tool": "pen",
                        "color": "#2864DC",
                        "width": 2.0,
                        "points": points,
                    }
                },
                "rect": {
                    "min": {"x": x, "y": y - 6.0},
                    "size": {"x": 63.0, "y": 12.0},
                },
            }
        )
    sheet["canvas"] = {"objects": objects}
    sheet["next_id"] = strokes + 52
    result = io.BytesIO()
    with zipfile.ZipFile(result, "w", zipfile.ZIP_DEFLATED) as archive:
        files = [
            ("manifest.json", manifest),
            ("content.json", json.dumps(workbook, separators=(",", ":"))),
        ]
        files.extend((f"resources/{digest}", blobs[digest]) for digest in sorted(blobs))
        for name, data in files:
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            if embedded_images:
                # Keep the new fixture's ZIP metadata host-neutral without changing the default.
                info.create_system = 0
            archive.writestr(info, data)
    return result.getvalue(), sum(map(len, blobs.values()))


def fixture(source: Path, destination: Path, strokes: int, embedded_images: bool = False) -> None:
    data, image_bytes = build_archive(source, strokes, embedded_images)
    destination.write_bytes(data)
    image_description = (
        f"{IMAGE_COUNT} embedded PNGs, {image_bytes} encoded image bytes"
        if embedded_images
        else f"{IMAGE_COUNT} image placeholders"
    )
    if embedded_images:
        image_description += f" (Python {'.'.join(map(str, sys.version_info[:3]))}, zlib {zlib.ZLIB_RUNTIME_VERSION})"
    print(f"{destination}: {strokes} strokes, {image_description}, {destination.stat().st_size} bytes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("directory", type=Path)
    parser.add_argument(
        "--embedded-images",
        action="store_true",
        help="use 50 distinct generated PNG resources (writes *-embedded.thinoda)",
    )
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=True)
    for count in (1000, 10000):
        suffix = "-embedded" if args.embedded_images else ""
        fixture(args.source, args.directory / f"board-{count}{suffix}.thinoda", count, args.embedded_images)
