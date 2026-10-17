// Local-only fixture for native browser smoke checks. Never used by the product.
const http = require('node:http');
const server = http.createServer((req, res) => {
  res.setHeader('Cache-Control', 'no-store');
  if (req.url.startsWith('/headers')) {
    res.setHeader('Content-Type', 'application/json');
    return res.end(JSON.stringify(req.headers));
  }
  res.setHeader('Content-Type', 'text/html; charset=utf-8');
  res.end(`<!doctype html><title>NoRAM test page</title><h1>Native browser fixture</h1><p id="path"></p><script>document.getElementById('path').textContent=location.pathname;</script><a href="/two">Second page</a><a href="/popup" target="_blank">Open new tab</a><input placeholder="Unsaved work"><button onclick="window.open('/scripted')">User popup</button>`);
});
server.listen(48621, '127.0.0.1', () => console.log('NoRAM QA fixture: http://127.0.0.1:48621'));
