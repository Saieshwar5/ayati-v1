import { Stack } from 'expo-router';
import * as SplashScreen from 'expo-splash-screen';
import { StatusBar } from 'expo-status-bar';
import { useEffect } from 'react';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { LiveProvider } from '../live/live-context';
import { SessionProvider, useSession } from '../session/session-context';
import { useTokens } from '../theme/use-tokens';

void SplashScreen.preventAutoHideAsync().catch(() => {});

/** Signed out: sign-in. Signed in: a workspace must be chosen before the tabs open. */
function RootNavigator() {
  const { state } = useSession();
  const t = useTokens();
  useEffect(() => {
    if (state.status !== 'loading') void SplashScreen.hideAsync().catch(() => {});
  }, [state.status]);
  if (state.status === 'loading') return null;
  const signedIn = state.status === 'signed-in';
  const inWorkspace = signedIn && !!state.workspace;
  // Keyed by person and workspace: switching drops every screen's data and stream.
  const key = signedIn ? `${state.session.user.id}/${state.workspace?.id ?? ''}` : 'signed-out';
  const stack = (
    <Stack key={key} screenOptions={{ headerShown: false, contentStyle: { backgroundColor: t.surface } }}>
      <Stack.Protected guard={inWorkspace}>
        <Stack.Screen name="(tabs)" />
        <Stack.Screen name="chat/[id]" />
        <Stack.Screen name="panel/[id]" />
        <Stack.Screen name="agent/[id]" />
      </Stack.Protected>
      <Stack.Protected guard={signedIn}>
        <Stack.Screen name="workspace" />
      </Stack.Protected>
      <Stack.Protected guard={!signedIn}>
        <Stack.Screen name="sign-in" />
      </Stack.Protected>
    </Stack>
  );
  return inWorkspace ? <LiveProvider key={key}>{stack}</LiveProvider> : stack;
}

export default function RootLayout() {
  return (
    <SafeAreaProvider>
      <SessionProvider>
        <StatusBar style="auto" />
        <RootNavigator />
      </SessionProvider>
    </SafeAreaProvider>
  );
}
