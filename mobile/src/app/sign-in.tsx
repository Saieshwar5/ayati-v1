/** Screen 1: sign in with the work account in the system browser. */
import { StyleSheet, Text, View } from 'react-native';
import { useSession } from '../session/session-context';
import { SignInCancelled } from '../session/sign-in-flow';
import { type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { Button, ErrorText } from '../ui/controls';
import { useMutation } from '../ui/requests';
import { Screen } from '../ui/screen';

export default function SignIn() {
  const t = useTokens();
  const { state, signIn } = useSession();
  const mutation = useMutation();
  const start = (google: boolean) =>
    mutation.run(async () => {
      try {
        await signIn({ google });
      } catch (error) {
        // Closing the browser is a choice, not an error.
        if (!(error instanceof SignInCancelled)) throw error;
      }
    });
  const notice = state.status === 'signed-out' ? state.error : undefined;
  return (
    <Screen>
      <View style={styles.body}>
        <Text accessibilityRole="header" style={[typography.largeTitle, { color: t.text }]}>
          Cuckoo
        </Text>
        <Text style={[typography.body, { color: t.muted }]}>Your team and its agents, in your pocket.</Text>
        <View style={styles.actions}>
          <Button label="Sign in with your work account" busy={mutation.busy} onPress={() => void start(false)} />
          <Button label="Continue with Google" kind="outline" disabled={mutation.busy} onPress={() => void start(true)} />
          <ErrorText>{mutation.error || notice}</ErrorText>
        </View>
        <Text style={[typography.meta, { color: t.muted }]}>Cuckoo never sees your password.</Text>
        <Text style={[typography.meta, { color: t.muted }]}>New here? Ask your admin for an invitation.</Text>
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  body: { flex: 1, justifyContent: 'center', paddingHorizontal: 24, gap: 12 },
  actions: { gap: 12, marginVertical: 20 },
});
