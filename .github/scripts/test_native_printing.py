"""Job-free native bridge integration checks.

Windows compiles/links real SDK contracts and exercises DXGI preview rendering
and lifecycle without opening a dialog, selecting a printer or creating a job.
macOS compiles/links against the installed SDK for Intel and ARM, without loading
the result. Linux requires GTK 3 development files and cc; the cancellation check
additionally requires xvfb-run. No test accepts a print dialog or submits a job.
"""

from __future__ import annotations

import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "crates/thinoda-platform/native/print_linux.c"


def compile_windows_probe(directory: Path, *, ui: bool = False) -> Path:
    vswhere = (
        Path(os.environ["ProgramFiles(x86)"])
        / "Microsoft Visual Studio/Installer/vswhere.exe"
    )
    installation = subprocess.check_output(
        [str(vswhere), "-latest", "-products", "*", "-property", "installationPath"],
        text=True,
    ).strip()
    if not installation:
        raise RuntimeError("MSVC installation required for native printing")
    setup = Path(installation) / "VC/Auxiliary/Build/vcvarsall.bat"
    architecture = "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"
    source = ROOT / "crates/thinoda-platform/native/tests/print_windows_test.cpp"
    executable = directory / ("preview-ui.exe" if ui else "preview-test.exe")
    define = "/DTHINODA_PREVIEW_UI_PROBE" if ui else ""
    command = (
        f'call "{setup}" {architecture} >nul && cl /nologo /std:c++17 '
        f'/EHsc /W4 /WX /permissive- /MD {define} "{source}" '
        f'/Fo"{directory / "preview-test.obj"}" /Fe"{executable}" '
        'windowsapp.lib d3d11.lib d2d1.lib windowscodecs.lib ole32.lib user32.lib'
    )
    subprocess.run(command, shell=True, check=True, timeout=120)
    return executable


@unittest.skipUnless(sys.platform == "win32", "requires Windows and the real Windows SDK")
class WindowsNativePrintingTests(unittest.TestCase):
    def test_job_free_preview_rendering_and_lifecycle(self):
        with tempfile.TemporaryDirectory(prefix="thinoda-win-preview-") as directory:
            executable = compile_windows_probe(Path(directory))
            subprocess.run([str(executable)], check=True, timeout=60)


@unittest.skipUnless(sys.platform == "darwin", "requires macOS and the real Apple SDK")
class MacNativePrintingTests(unittest.TestCase):
    def test_bridge_compiles_and_links_with_supported_sdk_for_both_architectures(self):
        # Real SDK declarations catch invented Objective-C properties/selectors;
        # linking also catches missing PrintCore/framework symbols. Neither a
        # window nor a printer is needed, and no compiled code is executed.
        sdk = subprocess.check_output(
            ["xcrun", "--sdk", "macosx", "--show-sdk-path"], text=True
        ).strip()
        source = ROOT / "crates/thinoda-platform/native/print_macos.m"
        with tempfile.TemporaryDirectory(prefix="thinoda-mac-print-compile-") as directory:
            for architecture in ("x86_64", "arm64"):
                with self.subTest(architecture=architecture):
                    subprocess.run(
                        [
                            "xcrun", "--sdk", "macosx", "clang",
                            "-arch", architecture, "-isysroot", sdk,
                            "-mmacosx-version-min=11.0", "-fobjc-arc",
                            "-Wall", "-Wextra", "-Werror",
                            "-Werror=unguarded-availability-new",
                            "-dynamiclib", str(source),
                            "-framework", "AppKit", "-framework", "ApplicationServices",
                            "-o", str(Path(directory) / f"print-{architecture}.dylib"),
                        ],
                        check=True, timeout=60,
                    )

# Runs in a subprocess so initialization, main-thread identity and display
# configuration are independent of other tests. The parent enforces a timeout.
PROBE = r"""
import ctypes as c
import sys

library = c.CDLL(sys.argv[1])
native_print = library.thinoda_print
native_print.argtypes = [
    c.c_void_p, c.c_int, c.c_void_p, c.c_int, c.c_int,
    c.c_double, c.c_double, c.c_int, c.c_char_p, c.c_void_p, c.c_size_t,
]
native_print.restype = c.c_int
pixels = (c.c_ubyte * 4)(255, 255, 255, 255)
error = c.create_string_buffer(1024)
mode = sys.argv[2]
if mode == "cancel":
    gtk = c.CDLL("libgtk-3.so.0")
    gtk.gtk_init_check.argtypes = [c.c_void_p, c.c_void_p]
    gtk.gtk_init_check.restype = c.c_int
    assert gtk.gtk_init_check(None, None), "GTK cannot initialize on Xvfb"

    class GList(c.Structure):
        pass
    GList._fields_ = [
        ("data", c.c_void_p), ("next", c.POINTER(GList)), ("prev", c.POINTER(GList)),
    ]
    gtk.gtk_window_list_toplevels.restype = c.POINTER(GList)
    gtk.gtk_widget_get_visible.argtypes = [c.c_void_p]
    gtk.gtk_widget_get_visible.restype = c.c_int
    gtk.gtk_dialog_get_type.restype = c.c_size_t
    gtk.g_type_check_instance_is_a.argtypes = [c.c_void_p, c.c_size_t]
    gtk.g_type_check_instance_is_a.restype = c.c_int
    gtk.gtk_dialog_response.argtypes = [c.c_void_p, c.c_int]
    gtk.g_list_free.argtypes = [c.c_void_p]
    callback_type = c.CFUNCTYPE(c.c_int, c.c_void_p)
    cancelled = []
    @callback_type
    def cancel_dialog(_):
        windows = gtk.gtk_window_list_toplevels()
        node = windows
        while node:
            window = node.contents.data
            if gtk.gtk_widget_get_visible(window) and gtk.g_type_check_instance_is_a(
                window, gtk.gtk_dialog_get_type()
            ):
                # GTK_RESPONSE_CANCEL only. Never GTK_RESPONSE_OK/APPLY/ACCEPT.
                gtk.gtk_dialog_response(window, -6)
                cancelled.append(True)
            node = node.contents.next
        gtk.g_list_free(windows)
        return 0 if cancelled else 1
    gtk.g_timeout_add.argtypes = [c.c_uint, callback_type, c.c_void_p]
    gtk.g_timeout_add.restype = c.c_uint
    gtk.g_timeout_add(100, cancel_dialog, None)

result = native_print(
    None, 3, pixels, 1, 1, 210.0, 297.0, 150,
    b"Thinoda cancellation test", error, len(error),
)
if mode == "cancel":
    assert cancelled, "No native GTK dialog was observed"
    assert result == 0, (result, error.value)
else:
    assert result == -1 and b"initialize GTK" in error.value, (result, error.value)
"""


def has_gtk_headers() -> bool:
    return (
        sys.platform.startswith("linux")
        and shutil.which("cc") is not None
        and shutil.which("pkg-config") is not None
        and subprocess.run(
            ["pkg-config", "--exists", "gtk+-3.0"],
            check=False,
        ).returncode == 0
    )


@unittest.skipUnless(has_gtk_headers(), "requires Linux, cc and GTK 3 headers")
class NativePrintingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="thinoda-native-print-test-")
        cls.addClassCleanup(cls.directory.cleanup)
        cls.library = str(Path(cls.directory.name) / "libthinoda-print.so")
        headers = subprocess.check_output(
            ["pkg-config", "--cflags", "gtk+-3.0"], text=True
        ).split()
        # No direct GTK link: packaging must not bundle a partial GTK stack.
        subprocess.run(
            [
                "cc", "-std=gnu11", "-Wall", "-Wextra", "-Werror",
                "-Wl,--no-undefined", "-shared", "-fPIC", str(SOURCE),
                "-o", cls.library, *headers, "-ldl", "-lm",
            ],
            check=True,
        )

    def test_missing_display_is_an_error_not_cancellation(self):
        environment = {
            **os.environ,
            "DISPLAY": "",
            "WAYLAND_DISPLAY": "",
            "GDK_BACKEND": "x11",
        }
        subprocess.run(
            [sys.executable, "-c", PROBE, self.library, "no-display"],
            env=environment, check=True, timeout=15,
        )

    @unittest.skipUnless(shutil.which("xvfb-run"), "requires xvfb-run")
    def test_real_gtk_printer_dialog_cancellation_never_submits(self):
        subprocess.run(
            ["xvfb-run", "-a", sys.executable, "-c", PROBE, self.library, "cancel"],
            env={**os.environ, "GDK_BACKEND": "x11"}, check=True, timeout=20,
        )


if __name__ == "__main__":
    unittest.main()
