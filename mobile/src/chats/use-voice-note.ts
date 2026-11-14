/**
 * Recording a voice note: the mic starts it, Send stops and shares it, and
 * × throws it away. It stops at 5 minutes, as on the web.
 */
import { AudioModule, RecordingPresets, setAudioModeAsync, useAudioRecorder, useAudioRecorderState } from 'expo-audio';
import { useCallback, useEffect, useState } from 'react';

export const MAX_SECONDS = 5 * 60;
const OPTIONS = { ...RecordingPresets.HIGH_QUALITY, isMeteringEnabled: true };

export type VoiceNote = { uri: string; seconds: number };

export function useVoiceNote() {
  const recorder = useAudioRecorder(OPTIONS);
  const state = useAudioRecorderState(recorder, 250);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState('');
  const seconds = Math.floor(state.durationMillis / 1000);

  const start = useCallback(async () => {
    setError('');
    const permission = await AudioModule.requestRecordingPermissionsAsync();
    if (!permission.granted) {
      setError('Allow microphone access in Settings to record a voice note.');
      return;
    }
    await setAudioModeAsync({ allowsRecording: true, playsInSilentMode: true });
    await recorder.prepareToRecordAsync();
    recorder.record({ forDuration: MAX_SECONDS });
    setRecording(true);
  }, [recorder]);

  /** Stop and return the recording, ready to share. */
  const finish = useCallback(async (): Promise<VoiceNote | undefined> => {
    if (!recording) return undefined;
    await recorder.stop();
    setRecording(false);
    await setAudioModeAsync({ allowsRecording: false, playsInSilentMode: true });
    return recorder.uri ? { uri: recorder.uri, seconds } : undefined;
  }, [recorder, recording, seconds]);

  const cancel = useCallback(async () => {
    if (!recording) return;
    await recorder.stop();
    setRecording(false);
    await setAudioModeAsync({ allowsRecording: false, playsInSilentMode: true });
  }, [recorder, recording]);

  // Leaving the conversation never keeps the microphone on.
  useEffect(
    () => () => {
      if (recorder.isRecording) void recorder.stop().catch(() => {});
    },
    [recorder],
  );

  return { recording, seconds, level: state.metering, error, start, finish, cancel };
}

export const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;
