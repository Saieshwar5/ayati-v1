/** The five tabs on the grey canvas (design principle 1). */
import { Tabs } from 'expo-router';
import { NeedsYouProvider, useNeedsYou } from '../../needs-me/needs-you-context';
import { useTokens } from '../../theme/use-tokens';
import { AgentsIcon, ChatsIcon, HomeIcon, NeedsMeIcon, YouIcon } from '../../ui/tab-icons';

export default function TabsLayout() {
  return (
    <NeedsYouProvider>
      <FiveTabs />
    </NeedsYouProvider>
  );
}

function FiveTabs() {
  const t = useTokens();
  const waiting = useNeedsYou().data?.items.length ?? 0;
  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: t.text,
        tabBarInactiveTintColor: t.muted,
        tabBarStyle: { backgroundColor: t.canvas, borderTopColor: t.border },
        sceneStyle: { backgroundColor: t.surface },
        tabBarBadgeStyle: { backgroundColor: t.danger, color: t.onPrimary },
        tabBarLabelStyle: { fontSize: 13, fontWeight: '600' },
      }}
    >
      <Tabs.Screen name="index" options={{ title: 'Home', tabBarIcon: HomeIcon }} />
      <Tabs.Screen name="chats" options={{ title: 'Chats', tabBarIcon: ChatsIcon }} />
      <Tabs.Screen
        name="needs-me"
        options={{
          title: 'Needs me',
          tabBarIcon: NeedsMeIcon,
          tabBarBadge: waiting ? waiting : undefined,
          tabBarAccessibilityLabel: waiting ? `Needs me, ${waiting} waiting` : 'Needs me',
        }}
      />
      <Tabs.Screen name="agents" options={{ title: 'Agents', tabBarIcon: AgentsIcon }} />
      <Tabs.Screen name="you" options={{ title: 'You', tabBarIcon: YouIcon }} />
    </Tabs>
  );
}
