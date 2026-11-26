import { useCallback, useRef, useState } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { FileName, FileTile } from '../results/files';
import { fileSize } from '../results/file-kind';
import { itemUrl } from '../library/model';
import './conversation-files.css';

export type ConversationFile = {
  id: string;
  name: string;
  mime: string;
  size: number;
  at: number;
  chat: string;
  chat_name: string;
  from?: string | null;
  made_by?: string | null;
  library_item?: string | null;
  /** False when shown only because the viewer keeps the agent. */
  mine?: boolean;
};
const base = `/workspaces/${workspaceId}`;
const day = (at: number) =>
  new Date(at * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });

/** One file: who it is from, a download, and keeping it in the Library. */
function FileRow({ file, changed }: { file: ConversationFile; changed: () => void }) {
  const mutation = useMutation();
  // One Library item per attempt, so a retried save never makes two.
  const attempt = useRef<string | undefined>(undefined);
  const open = file.mine !== false;
  return (
    <li className="conversation-file">
      <FileTile file={file} />
      <span className="chat-file-text">
        {open ? (
          <a href={`/api/identity${base}/objects/${encodeURIComponent(file.id)}`}>
            <FileName name={file.name} />
          </a>
        ) : (
          <FileName name={file.name} />
        )}
        <small>
          {file.made_by ? `Made by ${file.made_by}` : `From ${file.from ?? 'someone'}`} ·{' '}
          {day(file.at)} · {fileSize(file.size)}
        </small>
        {mutation.status && <small role="alert">{mutation.status}</small>}
      </span>
      {file.library_item ? (
        <a className="conversation-file-kept" href={itemUrl(file.library_item)}>
          In Library
        </a>
      ) : (
        open && (
          <Button
            disabled={mutation.busy}
            onClick={() =>
              void mutation.run(async () => {
                attempt.current ??= crypto.randomUUID();
                await identityRequest(`${base}/objects/${encodeURIComponent(file.id)}/keep`, {
                  id: attempt.current,
                  folder: null,
                });
                changed();
              })
            }
          >
            Save to Library
          </Button>
        )
      )}
    </li>
  );
}

/** The files of the open conversation: what people attached and what an agent made.
 * They are read when the section is opened, not with every look at Details. */
export function ConversationFiles({ chat }: { chat: string }) {
  const [open, setOpen] = useState(false);
  return (
    <section className="conversation-files" aria-label="Files in this conversation">
      <Button aria-expanded={open} onClick={() => setOpen(!open)}>
        Files
      </Button>
      {open && <FileList chat={chat} />}
    </section>
  );
}
function FileList({ chat }: { chat: string }) {
  const load = useCallback(
    () => identityRequest<{ files?: ConversationFile[] }>(`${base}/chats/${chat}/files`),
    [chat],
  );
  const files = useResource(load);
  const refresh = () => void files.refresh().catch(() => {});
  const listed = files.data?.files ?? [];
  return (
    <>
      <p className="chat-help">
        Only the people in this conversation can open these. Save one to the Library to keep it
        where others can find it.
      </p>
      {files.error && <p role="alert">{files.error}</p>}
      {files.data && listed.length === 0 && <p className="chat-help">No files yet.</p>}
      <ul>
        {listed.map((file) => (
          <FileRow key={file.id} file={file} changed={refresh} />
        ))}
      </ul>
    </>
  );
}

/** Files sent to an agent or made by it, grouped by conversation. */
export function AgentArtifacts({ agent, name }: { agent: string; name: string }) {
  const load = useCallback(
    () =>
      identityRequest<{ files?: ConversationFile[]; keeper?: boolean }>(
        `${base}/agent-profiles/${agent}/artifacts`,
      ),
    [agent],
  );
  const artifacts = useResource(load);
  const refresh = () => void artifacts.refresh().catch(() => {});
  const groups = new Map<string, ConversationFile[]>();
  for (const file of artifacts.data?.files ?? [])
    groups.set(file.chat, [...(groups.get(file.chat) ?? []), file]);
  return (
    <div className="conversation-files">
      <p className="ui-help">
        Files people sent {name} and files it made, by conversation. They stay with their
        conversation; the Library holds only what someone saved there.
        {artifacts.data?.keeper
          ? ` You look after ${name}, so you also see the names of files in conversations you are not in.`
          : ''}
      </p>
      {artifacts.error && <p role="alert">{artifacts.error}</p>}
      {artifacts.data && groups.size === 0 && <p className="ui-help">No files yet.</p>}
      {[...groups.entries()].map(([chat, files]) => (
        <section key={chat} aria-label={files[0].chat_name}>
          <h4>
            {files[0].chat_name}
            {files[0].mine === false && <small> · not your conversation</small>}
          </h4>
          <ul>
            {files.map((file) => (
              <FileRow key={file.id} file={file} changed={refresh} />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
