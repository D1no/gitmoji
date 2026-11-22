"""Shared local TLS server and certificate fixture for native hosting tests."""

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import ssl
import subprocess
import tempfile
import threading


class LocalTlsFixture:
    """Ephemeral CA + localhost certificate and HTTPS server for testing native gitmoji."""

    def __init__(self, handler_cls, ca_crt=None, ca_key=None):
        self._dir = tempfile.TemporaryDirectory(prefix="gitmoji-tls-")
        root = Path(self._dir.name)
        if ca_crt and ca_key:
            self.ca_crt = ca_crt
            self.ca_key = ca_key
        else:
            self.ca_crt = root / "ca.crt"
            self.ca_key = root / "ca.key"
            subprocess.run(
                [
                    "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                    "-keyout", str(self.ca_key), "-out", str(self.ca_crt),
                    "-days", "1", "-subj", "/CN=gitmoji-test-ca",
                ],
                check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
        srv_crt = root / "srv.crt"
        srv_key = root / "srv.key"
        srv_csr = root / "srv.csr"
        ext = root / "ext.cnf"

        subprocess.run(
            [
                "openssl", "req", "-newkey", "rsa:2048", "-nodes",
                "-keyout", str(srv_key), "-out", str(srv_csr),
                "-subj", "/CN=localhost",
            ],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        ext.write_text("subjectAltName=DNS:localhost,IP:127.0.0.1\n")
        subprocess.run(
            [
                "openssl", "x509", "-req", "-in", str(srv_csr),
                "-CA", str(self.ca_crt), "-CAkey", str(self.ca_key), "-CAcreateserial",
                "-out", str(srv_crt), "-days", "1", "-extfile", str(ext),
            ],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), handler_cls)
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(srv_crt, srv_key)
        self.server.socket = ctx.wrap_socket(self.server.socket, server_side=True)
        self.port = self.server.server_port
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self._dir.cleanup()


def read_request_body(handler: BaseHTTPRequestHandler) -> bytes:
    """Read a fixed-length or Transfer-Encoding: chunked HTTP request body."""
    te = handler.headers.get("Transfer-Encoding", "").lower()
    if "chunked" in te:
        chunks = []
        while True:
            line = handler.rfile.readline()
            if not line:
                break
            size = int(line.strip().split(b";", 1)[0], 16)
            if size == 0:
                while True:
                    trailer = handler.rfile.readline()
                    if not trailer or trailer in (b"\r\n", b"\n"):
                        break
                break
            chunks.append(handler.rfile.read(size))
            handler.rfile.read(2)
        return b"".join(chunks)
    length = int(handler.headers.get("Content-Length", "0") or "0")
    return handler.rfile.read(length) if length else b""


def serve_git_http(handler: BaseHTTPRequestHandler, repo_root: Path, git_path: str):
    """Handle a git smart-HTTP request via `git http-backend`."""
    body = read_request_body(handler)
    path_only, _, query = git_path.partition("?")
    env = {
        "GIT_PROJECT_ROOT": str(repo_root),
        "GIT_HTTP_EXPORT_ALL": "1",
        "PATH_INFO": path_only,
        "REQUEST_METHOD": handler.command,
        "QUERY_STRING": query,
        "REMOTE_ADDR": "127.0.0.1",
        "CONTENT_TYPE": handler.headers.get("Content-Type", ""),
        "CONTENT_LENGTH": str(len(body)),
        "PATH": "/usr/local/bin:/usr/bin:/bin",
    }
    proc = subprocess.run(
        ["git", "http-backend"],
        input=body, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, timeout=15,
    )
    if proc.returncode != 0:
        handler.send_response(500)
        handler.end_headers()
        handler.wfile.write(proc.stderr)
        return
    raw_head, _, payload = proc.stdout.partition(b"\r\n\r\n")
    if not payload and b"\n\n" in proc.stdout:
        raw_head, _, payload = proc.stdout.partition(b"\n\n")
    status = 200
    headers = []
    for line in raw_head.decode(errors="replace").splitlines():
        if ":" not in line:
            continue
        k, v = line.split(":", 1)
        if k.lower() == "status":
            status = int(v.strip().split()[0])
        else:
            headers.append((k.strip(), v.strip()))
    handler.send_response(status)
    for k, v in headers:
        handler.send_header(k, v)
    handler.send_header("Content-Length", str(len(payload)))
    handler.end_headers()
    handler.wfile.write(payload)
