import { createServer } from 'node:http';
import { once } from 'node:events';
import { performance } from 'node:perf_hooks';

const unixMicros = () => Math.round((performance.timeOrigin + performance.now()) * 1000);

// Follow the controller's explicit request ID in attributed transcripts. Keep
// legacy raw prompts for local-runtime and old queued-job compatibility tests.
export function contextCommand(text) {
  const marker = 'Conversation evidence (JSON):\n';
  const line = text.split(marker)[1]?.split('\n')[0];
  if (line) {
    const evidence = JSON.parse(line);
    const request = evidence.messages.find(message => message.id === evidence.request_message);
    if (!request) throw new Error('Fixture context is missing its explicit request');
    return request.content;
  }
  return text.split('\n\nAuthorized task sources')[0]
    .replace(/<turn-context>[\s\S]*?<\/turn-context>/g, '')
    .trim().split('\n').at(-1)?.replace(/^user:\s*/, '');
}

// A local model fixture, not live inference. The real pinned Goose process
// performs the tool loop, MCP calls, ACP permission requests and persistence.
export async function fixtureProvider({ resumableHold = false, key, heartbeat = false } = {}) {
  const requests = [];
  const controlScripts = new Map();
  let unavailable = false;
  let held = false;
  let heldMarkers = [];
  // Plain-text replies for prompts that are not control scripts (the front desk).
  const replies = new Map();
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
    const prompt = contextCommand(textContent);
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
    let text = replies.get(prompt) ?? 'GOOSE_FIXTURE_OK 🦀';
    const savedMatch = textContent.match(/Saved task continuation[^\n]*:\n(\{[^\n]+\})/);
    const savedTask = savedMatch ? JSON.parse(savedMatch[1]) : undefined;
    const originalLine = savedTask ? contextCommand(textContent.split('Saved task continuation')[0]) : prompt;
    // Automation messages contain a controller-generated title before the
    // configured instruction. Script the instruction, not that display title.
    const controlPrompt = originalLine
      ?.replace(/^(?:Connected event work|Scheduled work) · [^\n]*\n/, '')
      .replace(/^\[Personal Home panel (?:request|change)\]\n[^\n]*\n/, '');
    // A follow-up's first line ("Follow-up · <task title>") can key a script too.
    const scriptedCommand = controlScripts.get(originalLine) ?? controlScripts.get(controlPrompt) ?? controlScripts.get(originalLine?.split('\n')[0]);
    if (controlPrompt?.startsWith('permission_draft ')) {
      const setup = JSON.parse(controlPrompt.slice(17));
      text = JSON.stringify({ rules: setup.actions.filter(a => a.operation !== 'order.delete').map(a => ({ connection: a.connection, operation: a.operation, mode: a.effect === 'read' ? 'allow' : 'ask' })), warnings: ['Fixture suggestion: delete is omitted and must remain blocked.'] });
    }
    if (scriptedCommand || controlPrompt?.startsWith('control ')) {
      const command = scriptedCommand ?? JSON.parse(controlPrompt.slice(8));
      const results = messages.slice(lastUser + 1).filter(message => message.role === 'tool');
      const declined = savedTask?.questions?.some(question => ['declined', 'invalidated'].includes(question.state));
      const steps = savedTask ? (!declined && savedTask.actions.every(a => a.state === 'completed')
        ? savedTask.actions.length === 0 && command.afterQuestion ? command.afterQuestion : command.after ?? [] : []) : command.steps ?? [command];
      const step = steps[results.length];
      if (step) tool = { id: `control-${results.length}`, type: 'function', function: { name: `ayati_control__${step.tool}`, arguments: JSON.stringify(step.arguments) } };
      else text = savedTask ? command.questionCheckpoint && savedTask.actions.length === 0 && command.afterQuestion && !declined ? command.questionCheckpoint
        : `CONTINUED_RESULT ${JSON.stringify({handoffs:savedTask.handoffs?.map(h=>({key:h.key,state:h.state,result:h.result,truncated:h.truncated})),decisions:savedTask.actions.map(a=>({id:a.id,state:a.state,decision:a.decision})),questions:savedTask.questions?.map(q=>({key:q.key,state:q.state,values:q.values})),results:results.map(result=>result.content)})}` : command.checkpoint ?? `CONTROL_RESULT ${JSON.stringify(results.map(result => result.content))}`;
    }
    if (prompt?.startsWith('workspace ')) {
      const command = JSON.parse(prompt.slice(10));
      const result = messages.slice(lastUser + 1).find(message => message.role === 'tool');
      if (result) text = `WORKSPACE_RESULT ${JSON.stringify(result.content)}`;
      else tool = { id: 'workspace-call', type: 'function', function: { name: `ayati_workspace__${command.tool}`, arguments: JSON.stringify(command.arguments) } };
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
      else if (script && !replies.has(prompt)) {
        // Controlled partial replies for reconnect/cancel and timing journeys;
        // a reply scripted for its own prompt (setReply) is never held.
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
    setControl: (prompt, command) => controlScripts.set(prompt, structuredClone(command)),
    setReply: (prompt, text) => replies.set(prompt, text),
    timings,
    setStream: value => { streamScript=value; },
    setOutputHeld: value => { outputHeld=value; },
    setUnavailable: (value) => { unavailable = value; },
    setHeld: (value) => { held = value; },
    setHeldMarkers: markers => { heldMarkers = [...markers]; },
    close: () => { server.closeAllConnections(); server.close(); },
  };
}
