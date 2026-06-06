#!/usr/bin/env python3
import http.server
import os

os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'dist'))

class CORSHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Access-Control-Allow-Origin', '*')
        super().end_headers()

http.server.test(HandlerClass=CORSHandler, port=1420, bind='127.0.0.1')
