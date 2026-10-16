import { createServer } from 'node:http';
import { once } from 'node:events';

// A local model fixture, not live inference. The real pinned Goose process
// performs the tool loop, MCP calls, ACP permission requests and persistence.
export async function fixtureProvider() {
  const requests = [];
  let unavailable = false;
  const server = createServer(async (request, response) => {
    if (request.method === 'GET') {
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify({ data: [{ id: 'fixture', object: 'model', owned_by: 'fixture' }] }));
      return;
    }
    let raw = '';
    for await (const chunk of request) raw += chunk;
    const input = JSON.parse(raw);
    requests.push(input);
    const messages = input.messages ?? [];
    const lastUser = messages.findLastIndex((message) => message.role === 'user');
    const content = messages[lastUser]?.content;
    const textContent = typeof content === 'string' ? content
      : (content ?? []).filter((part) => part.type === 'text').map((part) => part.text).join('\n');
    // Goose may merge consecutive user inputs after a cancelled turn. Match
    // the latest fixture command, not an old "stall" still in that history.
    const prompt = textContent.replace(/<turn-context>[\s\S]*?<\/turn-context>/g, '').trim().split('\n').at(-1);
    if (unavailable) {
      response.writeHead(403, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ error: { message: 'PRIVATE_PROVIDER_FAILURE_BODY' } }));
      return;
    }
    if (prompt === 'stall') return; // Deliberately interrupted by Stop.
    let tool;
    let text = 'GOOSE_FIXTURE_OK 🦀';
    if (/^tool\b/.test(prompt ?? '')) {
      const result = messages.slice(lastUser + 1).find((message) => message.role === 'tool');
      if (result) text = `TOOL_RESULT ${JSON.stringify(result.content)}`;
      else tool = { id: 'fixture-call', type: 'function', function: { name: 'ayati_proof__get_demo_business', arguments: '{}' } };
    }
    if (prompt?.startsWith('browser ')) {
      const command = JSON.parse(prompt.slice(8));
      const results = messages.slice(lastUser + 1).filter((message) => message.role === 'tool');
      const steps = command.steps ?? [command];
      const step = steps[results.length];
      if (!step) text = `BROWSER_RESULT ${JSON.stringify(results.at(-1)?.content)}`;
      else {
        const arguments_ = { ...step.arguments };
        if (step.refName) {
          const snapshot = String(results.at(-1)?.content ?? '');
          const escaped = step.refName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
          const reference = snapshot.match(new RegExp(`"${escaped}"[^\\n]*?ref=(e[0-9]+)`));
          if (!reference) throw new Error(`Fixture could not find observed browser reference: ${step.refName}`);
          arguments_.selector = '@' + reference[1];
        }
        tool = { id: `browser-call-${results.length}`, type: 'function', function: {
          name: `ayati_browser__agent_browser_${step.tool}`, arguments: JSON.stringify(arguments_),
        } };
      }
    }
    const message = tool ? { role: 'assistant', content: null, tool_calls: [tool] } : { role: 'assistant', content: text };
    if (input.stream) {
      response.setHeader('Content-Type', 'text/event-stream');
      const delta = (value, reason = null) => response.write(`data: ${JSON.stringify({ id: 'fixture', object: 'chat.completion.chunk', model: 'fixture', choices: [{ index: 0, delta: value, finish_reason: reason }] })}\n\n`);
      if (tool) delta({ tool_calls: [{ index: 0, ...tool }] });
      else {
        delta({ role: 'assistant', content: text.slice(0, 8) });
        await new Promise((resolve) => setTimeout(resolve, 20));
        delta({ content: text.slice(8) });
      }
      delta({}, tool ? 'tool_calls' : 'stop');
      response.end('data: [DONE]\n\n');
    } else {
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify({ id: 'fixture', object: 'chat.completion', model: 'fixture', choices: [{ index: 0, message, finish_reason: tool ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 } }));
    }
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return {
    url: `http://127.0.0.1:${server.address().port}`,
    requests,
    setUnavailable: (value) => { unavailable = value; },
    close: () => { server.closeAllConnections(); server.close(); },
  };
}
