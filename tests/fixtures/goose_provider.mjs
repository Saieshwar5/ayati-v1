import { createServer } from 'node:http';
import { once } from 'node:events';
import { performance } from 'node:perf_hooks';

const unixMicros = () => Math.round((performance.timeOrigin + performance.now()) * 1000);

// A local model fixture, not live inference. The real pinned Goose process
// performs the tool loop, MCP calls, ACP permission requests and persistence.
export async function fixtureProvider({ resumableHold = false, key, heartbeat = false } = {}) {
  const requests = [];
  let unavailable = false;
  let held = false;
  let heldMarkers = [];
  let streamScript;
  let outputHeld = false;
  const timings = [];
  const server = createServer(async (request, response) => {
    if (key && request.headers.authorization !== 'Bearer ' + key) {
      response.writeHead(403).end();
      return;
    }
    const headersUs = unixMicros();
    if (request.method === 'GET') {
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify({ data: [{ id: 'fixture', object: 'model', owned_by: 'fixture' }] }));
      return;
    }
    let raw = '';
    for await (const chunk of request) raw += chunk;
    const bodyUs = unixMicros();
    const input = JSON.parse(raw);
    requests.push(input);
    const index=requests.length-1;
    timings.push({index,phase:'request_headers',time_us:headersUs});
    timings.push({index,phase:'request_body',time_us:bodyUs});
    timings.push({index,phase:'request',time_ms:Date.now(),time_us:unixMicros()});
    const script=streamScript;
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
    if (heartbeat && input.stream) {
      response.setHeader('Content-Type', 'text/event-stream');
      response.flushHeaders();
      const timer = setInterval(() => response.write(': fixture keepalive\n\n'), 500);
      response.once('close', () => clearInterval(timer));
      response.once('finish', () => clearInterval(timer));
    }
    if (prompt === 'stall' || (held && !resumableHold)) return; // Deliberately interrupted by Stop/worker loss.
    while ((held || heldMarkers.some(marker => textContent.includes(marker))) && !response.destroyed) await new Promise(resolve => setTimeout(resolve, 20));
    if (response.destroyed) return;
    let tool;
    let text = 'GOOSE_FIXTURE_OK 🦀';
    const controlPrompt = prompt?.replace(/^user:\s*/, '');
    if (controlPrompt?.startsWith('control ')) {
      const command = JSON.parse(controlPrompt.slice(8));
      const results = messages.slice(lastUser + 1).filter(message => message.role === 'tool');
      const steps = command.steps ?? [command];
      const step = steps[results.length];
      if (step) tool = { id: `control-${results.length}`, type: 'function', function: { name: `ayati_control__${step.tool}`, arguments: JSON.stringify(step.arguments) } };
      else text = `CONTROL_RESULT ${JSON.stringify(results.map(result => result.content))}`;
    }
    if (prompt?.startsWith('workspace ')) {
      const command = JSON.parse(prompt.slice(10));
      const result = messages.slice(lastUser + 1).find(message => message.role === 'tool');
      if (result) text = `WORKSPACE_RESULT ${JSON.stringify(result.content)}`;
      else tool = { id: 'workspace-call', type: 'function', function: { name: `ayati_workspace__${command.tool}`, arguments: JSON.stringify(command.arguments) } };
    }
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
    if (prompt?.startsWith('memory ')) {
      const command = JSON.parse(prompt.slice(7));
      const results = messages.slice(lastUser + 1).filter((message) => message.role === 'tool');
      const steps = command.steps ?? [command];
      const step = steps[results.length];
      if (!step) text = `MEMORY_RESULT ${JSON.stringify(results.at(-1)?.content)}`;
      else {
        const args = { ...step.arguments };
        if (step.useSource) {
          const sourceText = JSON.stringify(results.at(-1)?.content);
          const match = sourceText.match(/(?:\\"|")id(?:\\"|"): ?(?:\\"|")([a-f0-9-]{36})/);
          if (!match) throw new Error('Source result missing id: ' + sourceText);
          args.source_id = match[1];
        }
        tool = { id: `memory-call-${results.length}`, type: 'function', function: { name: `ayati_memory__memory_${step.tool}`, arguments: JSON.stringify(args) } };
      }
    }
    if (prompt === 'Please summarize the conversation history provided in the system prompt.') text = 'COMPACTION_FIXTURE_SUMMARY: Continue preparing the daily owner report. Retrieve report preferences from scoped memory before finishing.';
    const message = tool ? { role: 'assistant', content: null, tool_calls: [tool] } : { role: 'assistant', content: text };
    if (input.stream) {
      if (!response.headersSent) response.setHeader('Content-Type', 'text/event-stream');
      const delta = (value, reason = null) => response.write(`data: ${JSON.stringify({ id: 'fixture', object: 'chat.completion.chunk', model: 'fixture', choices: [{ index: 0, delta: value, finish_reason: reason }] })}\n\n`);
      if (tool) delta({ tool_calls: [{ index: 0, ...tool }] });
      else if (script) {
        // Controlled partial replies for reconnect/cancel and timing journeys.
        delta({role:'assistant',reasoning_content:'PRIVATE_REASONING_SENTINEL'});
        if(script.firstDelayMs)await new Promise(resolve=>setTimeout(resolve,script.firstDelayMs));
        for (let part=0;part<script.chunks.length && !response.destroyed;part++) {
          if (part) await new Promise(resolve=>setTimeout(resolve,script.delayMs ?? 40));
          if (part===(script.holdAfter ?? 1)) while(outputHeld && !response.destroyed) await new Promise(resolve=>setTimeout(resolve,20));
          if(response.destroyed) return;
          timings.push({index,phase:part===0?'first_token':'token',time_ms:Date.now()});
          delta({content:script.chunks[part]});
        }
      } else {
        delta({ role: 'assistant', content: text.slice(0, 8) });
        await new Promise((resolve) => setTimeout(resolve, 20));
        delta({ content: text.slice(8) });
      }
      delta({}, tool ? 'tool_calls' : 'stop');
      response.end('data: [DONE]\n\n');
      timings.push({index,phase:'complete',time_ms:Date.now()});
    } else {
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify({ id: 'fixture', object: 'chat.completion', model: 'fixture', choices: [{ index: 0, message, finish_reason: tool ? 'tool_calls' : 'stop' }], usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 } }));
    }
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return {
    key,
    url: `http://127.0.0.1:${server.address().port}`,
    requests,
    timings,
    setStream: value => { streamScript=value; },
    setOutputHeld: value => { outputHeld=value; },
    setUnavailable: (value) => { unavailable = value; },
    setHeld: (value) => { held = value; },
    setHeldMarkers: markers => { heldMarkers = [...markers]; },
    close: () => { server.closeAllConnections(); server.close(); },
  };
}
