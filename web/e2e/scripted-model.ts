import { createServer, type ServerResponse } from 'node:http';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';

type ModelMessage = { role: string; content?: string };
export type ProviderCall = { started_at: string; duration_ms: number; status: number; model: string };

export async function startScriptedModel() {
  let failNext = false;
  const calls: ProviderCall[] = [];
  const server = createServer(async (request, response) => {
    const started = performance.now();
    let call: ProviderCall | undefined;
    try {
      const chunks: Buffer[] = [];
      for await (const chunk of request) chunks.push(Buffer.from(chunk));
      const body = JSON.parse(Buffer.concat(chunks).toString()) as { model: string; messages: ModelMessage[] };
      call = { started_at: new Date().toISOString(), duration_ms: 0, status: 200, model: body.model };
      calls.push(call);
      if (failNext) {
        failNext = false;
        call.status = 503;
        response.writeHead(503, { 'content-type': 'application/json' });
        response.end(JSON.stringify({ error: 'synthetic-provider-body-DO-NOT-SHOW' }));
        return;
      }
      response.writeHead(200, { 'content-type': 'text/event-stream' });
      const user = body.messages.findLast((message) => message.role === 'user')?.content ?? '';
      const last = body.messages.at(-1);
      if (last?.role === 'tool') {
        await text(response, user.includes('expenses')
          ? 'Your expense summary is ready. Total: 550. Travel: 320, Food: 80, Supplies: 150.'
          : 'The revised notes are ready. Your original is preserved.');
      } else if (/what did|follow.up/i.test(user)) {
        const hasFiles = body.messages.some((message) => message.content?.includes('/workspace/'));
        await text(response, hasFiles ? 'We created a revised file and preserved your original.' : 'No file context found.');
      } else if (/literal markup/i.test(user)) {
        await text(response, '<img src=x onerror="window.ayatiInjected=true"><script>window.ayatiInjected=true</script>');
      } else if (/long explanation/i.test(user)) {
        await text(response, Array.from({ length: 80 }, (_, index) => `Useful explanation, line ${index + 1}.`).join('\n'));
      } else if (/long task/i.test(user)) {
        await tool(response, 'printf started > started.txt; (sleep 5; printf late > late.txt) & sleep 30; wait');
      } else {
        const path = user.match(/\/workspace\/[\w.-]+/)?.[0];
        if (!path) await text(response, 'I received your message.');
        else if (/expenses/i.test(user)) {
          await tool(response, `awk -F, 'NR>1 {s[$2]+=$3; total+=$3} END {print "category,amount"; for (c in s) print c "," s[c]; print "Total," total}' '${path}' > expense-summary.csv; cat expense-summary.csv`);
        } else await tool(response, `tr a-z A-Z < '${path}' > revised-notes.txt; cat revised-notes.txt`);
      }
    } catch {
      if (!response.headersSent) response.writeHead(500);
      response.end();
      if (call) call.status = 500;
    } finally {
      if (call) call.duration_ms = Math.round(performance.now() - started);
    }
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('No model fixture port');
  return {
    url: `http://127.0.0.1:${address.port}`, calls,
    failOnce: () => { failNext = true; },
    close: async () => {
      server.closeAllConnections();
      await new Promise<void>((done, reject) => server.close((error) => error ? reject(error) : done()));
    },
  };
}

function event(delta: object, finish_reason: string | null = null): string {
  return `data: ${JSON.stringify({ choices: [{ delta, finish_reason }] })}\n\n`;
}

async function text(response: ServerResponse, content: string): Promise<void> {
  const split = Math.min(24, Math.floor(content.length / 2));
  response.write(event({ content: content.slice(0, split) }));
  await delay(150);
  if (!response.destroyed) response.end(event({ content: content.slice(split) }, 'stop') + 'data: [DONE]\n\n');
}

async function tool(response: ServerResponse, command: string): Promise<void> {
  response.write(event({ content: 'I’ll inspect the file, create the result, and check it. ' }));
  await delay(150);
  const argumentsText = JSON.stringify({ command });
  const split = Math.floor(argumentsText.length / 2);
  response.write(event({ tool_calls: [{ index: 0, id: 'fixture_call', type: 'function',
    function: { name: 'shell', arguments: argumentsText.slice(0, split) } }] }));
  await delay(100);
  if (!response.destroyed) response.end(event({ tool_calls: [{ index: 0,
    function: { arguments: argumentsText.slice(split) } }] }, 'tool_calls') + 'data: [DONE]\n\n');
}
