import { useAppearance } from '../../stores/appearance.ts';
import { useSettings } from '../../stores/settings.ts';
import { vinylOn } from '../../theme/artwork.ts';

/** Whether covers are records right now (the user's choice, else the theme's). */
export function useVinyl(): boolean {
  const theme = useAppearance((s) => s.theme);
  const setting = useSettings((s) => s.settings.appearance.vinyl);
  return vinylOn(theme, setting);
}
