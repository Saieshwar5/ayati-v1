import { randomUUID } from 'node:crypto';

/** A repeating task of the app's signed-in person with its first run due at
 * `dueMs`: each run happens in `chat`, as that person (the one rule, schema 151).
 * Journeys that need work due at a time without asking the model first write it
 * here. Returns the task id. */
export async function dueTask(app, { chat = app.conversation, name, dueMs, instructions = 'Report once' }) {
  const w = app.path.split('/').at(-1), agent = app.profile.id;
  const person = (await app.request('/api/identity/session', app.cookie)).value.user.id;
  const task = randomUUID();
  const text = value => value.replaceAll("'", "''");
  app.sql(`INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,repeat,anchor_ms,instructions)
     VALUES('${w}','${task}','${agent}','commitment','person','${text(name)}','${person}','${chat}','{"unit":"hour"}',${dueMs},'${text(instructions)}');
    INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind) VALUES('${w}','${randomUUID()}','${task}',${dueMs},'repeat');`);
  return { task };
}
/** The reply of the task's run that started, once one has. */
export const startedRun = (app, task) => app.sql(`SELECT reply FROM relay_task_wakeups WHERE task='${task}' AND state='started' ORDER BY due_ms LIMIT 1`);
/** How many runs of the task have started. */
export const startedRuns = (app, task) => app.sql(`SELECT count(*) FROM relay_task_wakeups WHERE task='${task}' AND state='started'`);
