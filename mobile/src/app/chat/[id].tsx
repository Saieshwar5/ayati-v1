/** A conversation, full screen on a phone. */
import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { SafeAreaView } from 'react-native-safe-area-context';
import { ConversationView } from '../../chats/conversation-view';
import { useTokens } from '../../theme/use-tokens';

export default function ConversationScreen() {
  const t = useTokens();
  const router = useRouter();
  const { id } = useLocalSearchParams<{ id: string }>();
  return (
    <SafeAreaView edges={['top', 'left', 'right', 'bottom']} style={{ flex: 1, backgroundColor: t.surface }}>
      <Stack.Screen options={{ headerShown: false }} />
      <ConversationView key={id} id={id} onBack={() => router.back()} />
    </SafeAreaView>
  );
}
