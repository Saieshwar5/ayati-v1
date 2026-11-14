/** Simple line icons for the five tabs, drawn in the tab's tint. */
import type { ColorValue } from 'react-native';
import Svg, { Circle, Path, Rect } from 'react-native-svg';

type Icon = (props: { color: ColorValue; size: number }) => React.JSX.Element;
const line = { fill: 'none', strokeWidth: 1.8, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const };

export const HomeIcon: Icon = ({ color, size }) => (
  <Svg width={size} height={size} viewBox="0 0 24 24">
    <Path d="M4 10.5 12 4l8 6.5V19a1 1 0 0 1-1 1h-4.5v-5.5h-5V20H5a1 1 0 0 1-1-1z" stroke={color} {...line} />
  </Svg>
);

export const ChatsIcon: Icon = ({ color, size }) => (
  <Svg width={size} height={size} viewBox="0 0 24 24">
    <Path d="M5 5h14a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H10l-4.5 3.5V16H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1z" stroke={color} {...line} />
  </Svg>
);

export const NeedsMeIcon: Icon = ({ color, size }) => (
  <Svg width={size} height={size} viewBox="0 0 24 24">
    <Path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z" stroke={color} {...line} />
    <Path d="M10 20.5a2.2 2.2 0 0 0 4 0" stroke={color} {...line} />
  </Svg>
);

export const AgentsIcon: Icon = ({ color, size }) => (
  <Svg width={size} height={size} viewBox="0 0 24 24">
    <Rect x="5" y="7" width="14" height="12" rx="3" stroke={color} {...line} />
    <Path d="M12 7V4" stroke={color} {...line} />
    <Circle cx="9.5" cy="12.5" r="1.2" fill={color} />
    <Circle cx="14.5" cy="12.5" r="1.2" fill={color} />
    <Path d="M10 16h4" stroke={color} {...line} />
  </Svg>
);

export const YouIcon: Icon = ({ color, size }) => (
  <Svg width={size} height={size} viewBox="0 0 24 24">
    <Circle cx="12" cy="8.5" r="3.5" stroke={color} {...line} />
    <Path d="M5 20a7 7 0 0 1 14 0" stroke={color} {...line} />
  </Svg>
);
