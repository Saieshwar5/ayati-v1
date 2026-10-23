import { createServer } from 'node:http';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';

export async function scriptedUpstream() {
  const observed = [];
  const server = createServer(async (request, response) => {
    let raw = '';
    for await (const chunk of request) raw += chunk;
    const body = JSON.parse(raw);
    const prompt = JSON.stringify(body.messages);
    const marker = prompt.match(/CAL_[a-f0-9-]{36}/)?.[0];
    const long = prompt.includes('CAL_LONG') || prompt.includes('CAL_CANCEL');
    observed.push({ marker, max_tokens: body.max_tokens ?? body.max_completion_tokens,
      include_usage: body.stream_options?.include_usage, stream: body.stream });
    response.writeHead(200, { 'Content-Type': 'text/event-stream' });
    const frame = (choices, usage) => response.write('data: ' + JSON.stringify({
      id: 'synthetic', object: 'chat.completion.chunk', model: body.model, choices, ...(usage && { usage }),
    }) + '\n\n');
    const delta = (content, finish_reason = null) => frame([{ index: 0, delta: content, finish_reason }]);
    delta({ role: 'assistant', reasoning_content: 'PRIVATE_CALIBRATION_REASONING' });
    await delay(100);
    for (let i = 0; i < (long ? 30 : 3); i++) {
      if (response.destroyed) return;
      delta({ content: i === 0 ? 'Synthetic calibration answer 🦀' : ' continues.' });
      await delay(long ? 80 : 30);
    }
    delta({}, 'stop');
    frame([], { prompt_tokens: 120, completion_tokens: long ? 100 : 24, total_tokens: long ? 220 : 144 });
    response.end('data: [DONE]\n\n');
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return { url: `http://127.0.0.1:${server.address().port}`, observed,
    close: () => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }) };
}
