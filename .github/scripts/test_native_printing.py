"""Job-free Linux native bridge integration checks.

Requires GTK 3 development files and cc. The cancellation check additionally
requires xvfb-run. No test ever accepts a print dialog or submits a job.
"""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "crates/thinoda-platform/native/print_linux.c"

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
