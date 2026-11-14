/** A thin line under the header while the live connection is down (offline rule). */
import { Text, View } from 'react-native';
import { useLiveStatus } from '../live/live-context';
import { type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';

export function ConnectionBanner() {
  const t = useTokens();
  const status = useLiveStatus();
  if (status !== 'reconnecting') return null;
  return (
    <View accessibilityRole="alert" style={{ backgroundColor: t.soft, paddingHorizontal: 16, paddingVertical: 6 }}>
      <Text style={[typography.meta, { color: t.warning }]}>Reconnecting… new messages will appear when you are back online.</Text>
    </View>
  );
}
