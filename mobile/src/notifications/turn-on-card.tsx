/** Screen 3 as a card on Home: asked once, after sign-in, never at first launch. */
import { useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useSession } from '../session/session-context';
import { radius, type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { Button, ErrorText } from '../ui/controls';
import { useMutation } from '../ui/requests';
import { markAsked, shouldAsk, turnOn } from './push-setup';

export function TurnOnCard() {
  const t = useTokens();
  const { client } = useSession();
  const [show, setShow] = useState(false);
  const mutation = useMutation();
  useEffect(() => {
    let alive = true;
    void shouldAsk()
      .then(ask => alive && setShow(ask))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  if (!show) return null;
  const close = () => {
    void markAsked();
    setShow(false);
  };
  return (
    <View style={[styles.card, { borderColor: t.borderPanel, backgroundColor: t.surface }]}>
      <Text accessibilityRole="header" style={[typography.header, { color: t.text }]}>Turn on notifications</Text>
      <Text style={[typography.meta, { color: t.text }]}>
        Know when an agent replies, when an approval or a question waits for you, and when someone mentions you.
      </Text>
      <Text style={[typography.meta, { color: t.muted }]}>
        Notifications say who and what kind, never the message, files or amounts.
      </Text>
      <ErrorText>{mutation.error}</ErrorText>
      <View style={styles.buttons}>
        <Button label="Not now" kind="outline" disabled={mutation.busy} onPress={close} style={styles.button} />
        <Button label="Turn on" busy={mutation.busy} style={styles.button}
          onPress={() => void mutation.run(async () => {
            await turnOn(client);
            close();
          })} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  card: { borderWidth: 1, borderRadius: radius.card, padding: 16, gap: 8 },
  buttons: { flexDirection: 'row', gap: 10, marginTop: 4 },
  button: { flex: 1 },
});
