/**
 * The floating composer: + to attach, text, the mic and Send (screen 11).
 * Sending keeps one id per payload, so a retry after an unclear failure can't
 * post twice; a failed send keeps the draft and its files.
 */
import { filesOnlyContent, type Attachment, type Message, type Submission } from '@cuckoo/shared';
import * as Crypto from 'expo-crypto';
import { useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { radius, touchTarget, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { errorMessage } from '../ui/requests';
import { fileProblem, MAX_FILES, pickFile, pickPhoto, type Picked } from './attachments';
import { AttachRow, FileChips, RecordingRow } from './composer-tools';
import { useVoiceNote, type VoiceNote } from './use-voice-note';

type Props = {
  placeholder: string;
  disabled?: boolean;
  /** The agents this message asks: the direct conversation's agent, if any. */
  agents: string[];
  send: (submission: Submission) => Promise<{ message?: Message }>;
  sent: (message: Message) => void;
  /** Stores one chosen file in the conversation. */
  store: (picked: Picked) => Promise<Attachment>;
};

export function Composer({ placeholder, disabled, agents, send, sent, store }: Props) {
  const t = useTokens();
  const [draft, setDraft] = useState('');
  const [files, setFiles] = useState<Attachment[]>([]);
  const [attaching, setAttaching] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const pending = useRef(false);
  const attempt = useRef<Submission | undefined>(undefined);
  // A finished recording, and its stored file, kept until sent so Retry loses nothing.
  const voiceNote = useRef<VoiceNote | undefined>(undefined);
  const voiceFile = useRef<Attachment | undefined>(undefined);
  const voice = useVoiceNote();

  const submit = async (extra: Attachment[] = []) => {
    const attachments = [...files, ...extra].map(file => file.id);
    const content = draft.trim() || (attachments.length ? filesOnlyContent : '');
    if (!content || disabled) return;
    const asked = agents.length ? agents : undefined;
    // The same text, agents and files keep their id across retries.
    const same =
      attempt.current &&
      attempt.current.content === content &&
      attempt.current.agents?.join() === asked?.join() &&
      attempt.current.attachments.join() === attachments.join();
    if (!same) attempt.current = { id: Crypto.randomUUID(), content, agents: asked, attachments };
    const result = await send(attempt.current!);
    setDraft('');
    setFiles([]);
    voiceNote.current = undefined;
    voiceFile.current = undefined;
    attempt.current = undefined;
    if (result.message) sent(result.message);
  };

  const run = async (action: () => Promise<void>, failure: string) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (problem) {
      setError(`${failure} · ${errorMessage(problem)}`);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const sendNow = () =>
    run(async () => {
      if (voice.recording) {
        const note = await voice.finish();
        if (note) voiceNote.current = note;
      }
      if (voiceNote.current && !voiceFile.current) {
        voiceFile.current = await store({ uri: voiceNote.current.uri, name: 'voice-note.m4a', mime: 'audio/mp4', size: 0 });
      }
      await submit(voiceFile.current ? [voiceFile.current] : []);
    }, voice.recording || voiceNote.current ? 'Voice note not sent' : 'Not sent');

  const attach = (what: 'camera' | 'library' | 'file') =>
    run(async () => {
      const picked = what === 'file' ? await pickFile() : await pickPhoto(what);
      if (!picked) return;
      const problem = fileProblem(picked, files.length);
      if (problem) throw new Error(problem);
      const stored = await store(picked);
      setFiles(current => [...current, stored]);
      setAttaching(false);
    }, 'Not attached');

  const canSend = !disabled && !busy && (voice.recording || !!draft.trim() || files.length > 0 || !!voiceNote.current);
  return (
    <View style={styles.wrap}>
      {error || voice.error ? (
        <Pressable accessibilityRole="button" accessibilityLabel="Retry sending" disabled={busy} onPress={() => void sendNow()}>
          <Text accessibilityRole="alert" style={[typography.meta, styles.error, { color: t.danger }]}>
            {error || voice.error}
            {error.startsWith('Not sent') || error.startsWith('Voice') ? ' · Retry' : ''}
          </Text>
        </Pressable>
      ) : null}
      {voiceNote.current && !voice.recording && !busy ? (
        <Pressable accessibilityRole="button" accessibilityLabel="Discard the voice note" onPress={() => {
          voiceNote.current = undefined;
          voiceFile.current = undefined;
          setError('');
        }}>
          <Text style={[typography.meta, styles.error, { color: t.muted }]}>Voice note waiting · Discard</Text>
        </Pressable>
      ) : null}
      <FileChips files={files} disabled={busy} remove={id => setFiles(current => current.filter(file => file.id !== id))} />
      {attaching && !voice.recording ? <AttachRow disabled={busy || files.length >= MAX_FILES} choose={what => void attach(what)} /> : null}
      <View style={styles.line}>
        {voice.recording ? (
          <RecordingRow seconds={voice.seconds} level={voice.level} cancel={() => void voice.cancel()} />
        ) : (
          <View style={[styles.composer, { backgroundColor: t.raised, borderColor: t.borderControl, shadowColor: t.text }]}>
            <Pressable accessibilityRole="button" accessibilityLabel={attaching ? 'Hide attachments' : 'Attach a photo or file'}
              accessibilityState={{ expanded: attaching }} disabled={disabled} onPress={() => setAttaching(!attaching)} style={styles.icon}>
              <Text style={{ color: t.text, fontSize: 22 }}>{attaching ? '×' : '+'}</Text>
            </Pressable>
            <TextInput
              accessibilityLabel="Message"
              placeholder={placeholder}
              placeholderTextColor={t.muted}
              value={draft}
              onChangeText={setDraft}
              editable={!disabled}
              multiline
              maxLength={20000}
              style={[typography.body, styles.input, { color: t.text }]}
            />
            {!draft.trim() && !files.length ? (
              <Pressable accessibilityRole="button" accessibilityLabel="Record a voice note" disabled={disabled || busy}
                onPress={() => void voice.start()} style={styles.icon}>
                <Text style={{ color: t.text, fontSize: 18 }}>🎙</Text>
              </Pressable>
            ) : null}
          </View>
        )}
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={voice.recording ? 'Send the voice note' : 'Send message'}
          accessibilityState={{ disabled: !canSend, busy }}
          disabled={!canSend}
          onPress={() => void sendNow()}
          style={[styles.send, { backgroundColor: canSend ? t.primary : t.selection }]}
        >
          <Text style={{ color: canSend ? t.onPrimary : t.muted, fontWeight: '700' }}>↑</Text>
        </Pressable>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  wrap: { paddingHorizontal: 12, paddingTop: 6, paddingBottom: 8, gap: 6 },
  error: { paddingHorizontal: 6 },
  line: { flexDirection: 'row', alignItems: 'flex-end', gap: 8 },
  composer: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'flex-end',
    borderWidth: 1,
    borderRadius: radius.bubble,
    paddingHorizontal: 4,
    shadowOpacity: 0.08,
    shadowRadius: 12,
    shadowOffset: { width: 0, height: 4 },
    elevation: 2,
  },
  icon: { width: touchTarget, height: touchTarget, alignItems: 'center', justifyContent: 'center' },
  input: { flex: 1, maxHeight: 140, paddingVertical: 10 },
  send: { width: touchTarget, height: touchTarget, borderRadius: touchTarget / 2, alignItems: 'center', justifyContent: 'center' },
});
