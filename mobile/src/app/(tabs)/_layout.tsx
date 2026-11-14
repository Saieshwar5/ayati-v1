/** The five tabs on the grey canvas (design principle 1). */
import { Tabs } from 'expo-router';
import { useTokens } from '../../theme/use-tokens';

export default function TabsLayout() {
  const t = useTokens();
  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: t.text,
        tabBarInactiveTintColor: t.muted,
        tabBarStyle: { backgroundColor: t.canvas, borderTopColor: t.border },
        sceneStyle: { backgroundColor: t.surface },
        tabBarIconStyle: { display: 'none' },
        tabBarLabelStyle: { fontSize: 13, fontWeight: '600' },
      }}
    >
      <Tabs.Screen name="index" options={{ title: 'Home' }} />
      <Tabs.Screen name="chats" options={{ title: 'Chats' }} />
      <Tabs.Screen name="needs-me" options={{ title: 'Needs me' }} />
      <Tabs.Screen name="agents" options={{ title: 'Agents' }} />
      <Tabs.Screen name="you" options={{ title: 'You' }} />
    </Tabs>
  );
}
