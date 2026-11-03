import argparse
import copy
import json
import math
import zipfile
from pathlib import Path


def fixture(source: Path, destination: Path, strokes: int) -> None:
    with zipfile.ZipFile(source) as archive:
        manifest = archive.read("manifest.json")
        workbook = json.loads(archive.read("content.json"))

    sheet = workbook["sheets"][0]
    root = copy.deepcopy(sheet["topics"][str(sheet["root"])])
    prototype = next(topic for topic in sheet["topics"].values() if topic.get("card"))
    sheet["title"] = f"Performance {strokes}"
    sheet["root_mode"] = "hidden"
    sheet["topics"] = {str(root["id"]): root}
    sheet["floating"] = []
    sheet["zones"] = []
    for index in range(50):
        topic = copy.deepcopy(prototype)
        topic["id"] = index + 2
        topic["title"] = f"Image {index}"
        topic["position"] = {"x": float(index * 160), "y": -180.0}
        topic["card"] = {"kind": "image", "size": {"x": 140.0, "y": 100.0}}
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
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in (
            ("manifest.json", manifest),
            ("content.json", json.dumps(workbook, separators=(",", ":"))),
        ):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    print(f"{destination}: {strokes} strokes, 50 image placeholders, {destination.stat().st_size} bytes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=True)
    for count in (1000, 10000):
        fixture(args.source, args.directory / f"board-{count}.thinoda", count)
