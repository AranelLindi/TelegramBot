from http.server import BaseHTTPRequestHandler, HTTPServer
import json

class SimpleHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/sensors":
            data = json.dumps({"temperature": 26.3, "humidity": 55.0})
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))  # ✅ Wichtig!
            self.send_header("Connection", "close")  # ✅ Verbindung schließen
            self.end_headers()
            self.wfile.write(data.encode("utf-8"))

server = HTTPServer(("localhost", 8080), SimpleHandler)
print("🌍 Server läuft auf Port 8080...")
server.serve_forever()
