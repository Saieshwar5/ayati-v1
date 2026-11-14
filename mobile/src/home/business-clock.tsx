/** The workspace's clock as a small soft chip, as on the web's Home. */
import { useEffect, useState } from 'react';
import { Text, View } from 'react-native';
import { type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';

export function clockText(now: number, timezone?: string | null): string {
  const options: Intl.DateTimeFormatOptions = { hour: 'numeric', minute: '2-digit', timeZoneName: 'short' };
  try {
    return new Date(now).toLocaleTimeString(undefined, timezone ? { ...options, timeZone: timezone } : options);
  } catch {
    return new Date(now).toLocaleTimeString(undefined, options);
  }
}

export function BusinessClock({ timezone }: { timezone?: string | null }) {
  const t = useTokens();
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);
  return (
    <View style={{ alignSelf: 'flex-start', backgroundColor: t.soft, borderRadius: 10, paddingHorizontal: 10, paddingVertical: 4 }}>
      <Text accessibilityLabel={`Business time ${clockText(now, timezone)}`} style={[typography.meta, { color: t.text }]}>
        {clockText(now, timezone)}
      </Text>
    </View>
  );
}
