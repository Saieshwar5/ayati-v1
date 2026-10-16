import { createServer } from 'node:http';
import { once } from 'node:events';

export async function browserSite() {
  const writes = [];
  const requests = [];
  const server = createServer(async (request, response) => {
    requests.push({ path: request.url, cookie: request.headers.cookie ?? '' });
    response.setHeader('Cache-Control', 'no-store');
    if (request.url === '/download') {
      response.setHeader('Content-Type', 'application/octet-stream');
      response.setHeader('Content-Disposition', 'attachment; filename="fixture.txt"');
      response.end('Synthetic browser transfer'); return;
    }
    if (request.url === '/write' && request.method === 'POST') {
      let body = ''; for await (const chunk of request) body += chunk;
      writes.push(body);
      if (body.includes('lost-reply')) { request.socket.destroy(); return; }
      response.setHeader('Set-Cookie', 'ayati_fixture=logged-in; Path=/; Max-Age=86400; HttpOnly; SameSite=Lax');
      response.end('<h1>Saved once</h1>'); return;
    }
    if (request.url === '/redirect-private') { response.writeHead(302, { Location: 'http://169.254.169.254/' }); response.end(); return; }
    if (request.url === '/popup') { response.end('<h1>Popup page</h1>'); return; }
    response.setHeader('Content-Type', 'text/html');
    const login = request.url === '/login';
    response.end(`<!doctype html><html><head><title>Ayati browser fixture</title><style>body{font:20px sans-serif;margin:30px}h1{margin:0 0 20px;font-size:32px}label{display:block}input,button,a{display:block;margin:16px 0;font:20px sans-serif}input{height:40px}</style></head><body>
      <h1>Synthetic shop</h1><form action="/write" method="post">
      <label>Reference<input name="reference" aria-label="Reference"></label>
      ${login ? '<label>Password<input name="password" type="password" aria-label="Password"></label>' : ''}
      <button>Save once</button></form><a href="/download">Download sample</a>
      <label>Upload sample<input type="file" aria-label="Upload sample"></label>
      <a href="/popup" target="_blank">Open popup</a>
      <p id="cookie">${request.headers.cookie ?? 'No saved login'}</p>
      <div style="margin-top:1500px">End of page</div></body></html>`);
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  return { port: server.address().port, writes, requests,
    close: () => { server.closeAllConnections(); server.close(); } };
}
