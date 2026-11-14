import { useColorScheme } from 'react-native';
import { dark, light, type Tokens } from './tokens';

/** The tokens for the phone's light or dark appearance. */
export function useTokens(): Tokens {
  return useColorScheme() === 'dark' ? dark : light;
}
