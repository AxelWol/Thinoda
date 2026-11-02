import functools
import http.server
import os


class TestRequestHandler(http.server.SimpleHTTPRequestHandler):
    def log_request(self, code="-", size="-"):
        if not isinstance(code, int) or code >= 400:
            super().log_request(code, size)


if __name__ == "__main__":
    handler = functools.partial(
        TestRequestHandler,
        directory=os.environ.get("THINODA_BROWSER_DIST", "dist"),
    )
    http.server.ThreadingHTTPServer(("127.0.0.1", 8080), handler).serve_forever()
