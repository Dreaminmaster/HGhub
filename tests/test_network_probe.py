import importlib.util
import os
import pathlib
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("network_probe", pathlib.Path(__file__).resolve().parents[1] / "tools" / "network_probe.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ProbeTests(unittest.TestCase):
    def test_real_head_statuses_and_no_follow(self):
        seen = []
        class Handler(BaseHTTPRequestHandler):
            def do_HEAD(self):
                seen.append(self.path)
                status = int(self.path[1:])
                self.send_response(status)
                if status == 302:
                    self.send_header("Location", "/200")
                self.end_headers()
            def log_message(self, *args):
                pass
        with HTTPServer(("127.0.0.1",0),Handler) as server:
            t = threading.Thread(target=server.serve_forever, daemon=True)
            t.start()
            try:
                with patch.dict(os.environ, {"NO_PROXY":"127.0.0.1","no_proxy":"127.0.0.1"}):
                    for status, category in [(200,"http_reachable"),(401,"access_denied"),
                            (403,"access_denied"),(429,"rate_limited"),(503,"server_error"),
                            (405,"head_not_supported"),(302,"redirect_not_followed")]:
                        report = module.probe("http://127.0.0.1:%d/%d"%(server.server_port,status))
                        self.assertEqual(report["category"],category)
                        self.assertEqual(report["http_status"],status)
                        self.assertEqual(set(report),{"category","http_status","elapsed_ms"})
            finally:
                server.shutdown()
                t.join(timeout=3)
        self.assertEqual(seen,["/200","/401","/403","/429","/503","/405","/302"])

    def test_transport_error_classification(self):
        for code, expected in [(5,"proxy_dns"),(6,"host_dns"),(7,"connection_failed"),
                               (28,"timeout"),(35,"tls_handshake"),(60,"certificate_rejected")]:
            self.assertEqual(module.classify(code,0),expected)

    def test_rejects_unsafe_url_forms(self):
        for url in ["file:///tmp/test","https://user:secret@example.org/",
                    "https://example.org/?token=secret","https://example.org/#secret",
                    "https://example.org:99999/"]:
            with self.assertRaises(ValueError):
                module.validate(url)


if __name__ == "__main__":
    unittest.main()
