/**
 * Round faces for people and agents, as on the web (D253): agents show their
 * uploaded picture or a bundled robot; people show their photo or initials.
 * Uploaded pictures are private, so they are requested with the device token.
 */
import { agentPicture, defaultRobot, initials, personPicture, type Picture } from '@cuckoo/shared';
import { memo, useState } from 'react';
import { Image, StyleSheet, Text, View } from 'react-native';
import { SvgXml } from 'react-native-svg';
import { origin } from '../config';
import { useSession } from '../session/session-context';
import { useTokens } from '../theme/use-tokens';
import { robots } from './robots';

export type FaceIdentity = {
  name: string;
  agent?: { id: string; avatar?: string | null };
  person?: { id: string; photo?: string | null };
};

type Props = FaceIdentity & { size?: number; workspace: string };

export const Face = memo(function Face({ name, agent, person, size = 40, workspace }: Props) {
  const t = useTokens();
  const { token } = useSession();
  const [failed, setFailed] = useState(false);
  let picture: Picture = agent
    ? agentPicture(workspace, agent)
    : person
      ? personPicture(workspace, { ...person, name })
      : { kind: 'initials', text: initials(name) };
  // A picture that can't load falls back to what the web would show next.
  if (failed && picture.kind === 'image') {
    picture = agent ? { kind: 'robot', robot: defaultRobot(agent.id) } : { kind: 'initials', text: initials(name) };
  }
  const round = { width: size, height: size, borderRadius: size / 2 };
  return (
    <View
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={[styles.face, round, { backgroundColor: t.soft, borderColor: t.border }]}
    >
      {picture.kind === 'robot' && robots[picture.robot] ? (
        <SvgXml xml={robots[picture.robot]} width={size} height={size} />
      ) : picture.kind === 'image' ? (
        <Image
          source={{
            uri: `${origin}/api/identity${picture.path}`,
            headers: { Authorization: `Bearer ${token() ?? ''}`, 'X-Ayati-Request': 'identity' },
          }}
          onError={() => setFailed(true)}
          style={round}
        />
      ) : (
        <Text style={{ color: t.text, fontWeight: '600', fontSize: size * 0.38 }}>
          {picture.kind === 'initials' ? picture.text : '?'}
        </Text>
      )}
    </View>
  );
});

const styles = StyleSheet.create({
  face: { overflow: 'hidden', alignItems: 'center', justifyContent: 'center', borderWidth: StyleSheet.hairlineWidth },
});
